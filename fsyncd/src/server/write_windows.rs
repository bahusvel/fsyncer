use common::*;
use libc::uint32_t;
use server::{dokan::*, post_op, pre_op, SERVER_PATH};
use std::borrow::Cow;
use std::fs::symlink_metadata;
use std::io::ErrorKind;
use std::ptr;
use std::slice;
use winapi::um::fileapi::*;
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
use winapi::um::winnt::{FILE_SHARE_READ, PSID, SECURITY_DESCRIPTOR};

macro_rules! flagset {
    ($val:expr, $flag:expr) => {
        $val & $flag == $flag
    };
}

fn as_user<O, F: (FnOnce() -> O)>(handle: HANDLE, f: F) -> O {
    use winapi::um::securitybaseapi::{ImpersonateLoggedOnUser, RevertToSelf};
    if ImpersonateLoggedOnUser(handle) == 0 {
        panic!("Error impersonating logged on user");
    }
    let res = f();
    RevertToSelf();
    CloseHandle(handle);
    res
}

#[no_mangle]
pub unsafe extern "stdcall" fn MirrorCreateFile(
    path: LPCWSTR,
    context: PDOKAN_IO_SECURITY_CONTEXT,
    access: ACCESS_MASK,
    attributes: ULONG,
    mut shared: ULONG,
    disposition: ULONG,
    options: ULONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    use winapi::shared::ntstatus::STATUS_FILE_IS_A_DIRECTORY;

    let rpath = wstr_to_path(path);
    let rrpath = translate_path(&rpath, SERVER_PATH.as_ref().unwrap());
    let real_path = path_to_wstr(&rrpath);

    let attr = symlink_metadata(rrpath);
    let exists = !(attr.is_err() && attr.unwrap_err().kind() == ErrorKind::NotFound);
    // File exists and we need to open it

    let mut userAccess = 0 as ACCESS_MASK;
    let mut userAttributes = 0 as DWORD;
    let mut userDisposition = 0 as DWORD;
    let secDesc = (*context).AccessState.SecurityDescriptor;

    let userHandle = DokanOpenRequestorToken(info);
    assert!(userHandle != INVALID_HANDLE_VALUE);

    DokanMapKernelToUserCreateFileFlags(
        access,
        attributes,
        options,
        disposition,
        &mut userAccess as *mut _,
        &mut userAttributes as *mut _,
        &mut userDisposition as *mut _,
    );

    if exists && attr.unwrap().is_dir() {
        if !flagset!(options, FILE_NON_DIRECTORY_FILE) {
            (*info).IsDirectory = 1;
            shared |= FILE_SHARE_READ;
        } else {
            return STATUS_FILE_IS_A_DIRECTORY;
        }
    }

    let call;
    let status;
    let desc = secDesc as *const SECURITY_DESCRIPTOR;
    let security = FileSecurity::Windows {
        owner: {
            let (_, acc_name) = lookup_account((*desc).Owner).expect("Unknown account");
            Some(acc_name)
        },
        group: {
            let (_, acc_name) = lookup_account((*desc).Group).expect("Unknown account");
            Some(acc_name)
        },
        dacl: None,
    };

    if !exists
        && (flagset!(disposition, CREATE_ALWAYS) || flagset!(disposition, CREATE_NEW))
        && (*info).IsDirectory != 0
    {
        // Path does not exist, need to create it, and it is a directory
        call = Some(VFSCall::mkdir(mkdir {
            path: Cow::Borrowed(&rpath),
            security,
            mode: userAttributes,
        }));
        if let Some(r) = pre_op(call.as_ref().unwrap()) {
            return r;
        }
        status = as_user(userHandle, || {
            OpCreateDirectory(
                path,
                secDesc,
                userAccess,
                userAttributes,
                shared,
                userDisposition,
                &mut (*info).Context as *mut _ as *mut _,
            )
        });
    }

    if exists && (flagset!(disposition, TRUNCATE_EXISTING) || flagset!(disposition, CREATE_ALWAYS))
    {
        call = Some(VFSCall::truncate(truncate {
            path: Cow::Borrowed(&rpath),
            size: 0,
        }));
    } else if !exists && (flagset!(disposition, CREATE_ALWAYS) || flagset!(disposition, CREATE_NEW))
    {
        call = Some(VFSCall::create(create {
            path: Cow::Borrowed(&rpath),
            security,
            mode: userAttributes,
            flags: 0,
        }));
    }

    if call.is_some() {
        if let Some(r) = pre_op(call.as_ref().unwrap()) {
            return r;
        }
    }
    status = as_user(userHandle, || {
        OpCreateFile(
            real_path.as_ptr(),
            secDesc,
            userAccess,
            userAttributes,
            shared,
            userDisposition,
            &mut (*info).Context as *mut _ as *mut _,
        )
    });
    if call.is_some() {
        return post_op(call.as_ref().unwrap(), status);
    } else {
        return status;
    }
}

#[no_mangle]
pub unsafe extern "stdcall" fn MirrorCleanup(path: LPCWSTR, info: PDOKAN_FILE_INFO) {
    let real_path = trans_ppath!(path);
    let handle = (*info).Context as HANDLE;
    if !handle.is_null() {
        CloseHandle(handle);
        (*info).Context = 0;
    }
    let call;
    let status;
    if (*info).IsDirectory != 0 {
        call = VFSCall::rmdir(rmdir {
            path: Cow::Owned(wstr_to_path(path)),
        });
        if let Some(r) = pre_op(&call) {
            return;
        }
        status = OpDeleteDirectory(real_path.as_ptr());
    } else {
        call = VFSCall::unlink(unlink {
            path: Cow::Owned(wstr_to_path(path)),
        });
        if let Some(r) = pre_op(&call) {
            return;
        }
        status = OpDeleteFile(real_path.as_ptr());
    }
    post_op(&call, status);
}

#[no_mangle]
pub unsafe extern "stdcall" fn MirrorWriteFile(
    path: LPCWSTR,
    buffer: LPCVOID,
    len: DWORD,
    bytes_written: LPDWORD,
    offset: LONGLONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let rpath = wstr_to_path(path);
    let rrpath = translate_path(&rpath, SERVER_PATH.as_ref().unwrap());
    let real_path = path_to_wstr(&rrpath);

    let stat = symlink_metadata(rrpath);
    if stat.is_err() {
        return DokanNtStatusFromWin32(GetLastError());
    }
    let fileSize = stat.unwrap().len();

    let distanceToMove: u64;
    if (*info).WriteToEndOfFile != 0 {
        offset = fileSize as i64;
    } else {
        // Paging IO cannot write after allocate file size.
        if (*info).PagingIo != 0 {
            if offset as u64 >= fileSize {
                *bytes_written = 0;
                return STATUS_SUCCESS;
            }
            if (offset as u64 + len as u64) > fileSize {
                let bytes = fileSize - offset as u64;
                if (bytes >> 32) != 0 {
                    len = (bytes & 0xFFFFFFFF) as u32;
                } else {
                    len = bytes as u32;
                }
            }
        }
        if offset as u64 > fileSize {
            // In the mirror sample helperZeroFileData is not necessary. NTFS
            // will zero a hole. But if user's file system is different from
            // NTFS( or other Windows's file systems ) then  users will have to
            // zero the hole themselves.
        }
    }

    let call = VFSCall::write(write {
        path: Cow::Owned(rpath),
        buf: Cow::Borrowed(slice::from_raw_parts(buffer as *const u8, len as usize)),
        offset,
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = OpWriteFile(
        real_path.as_ptr(),
        buffer,
        len,
        bytes_written,
        offset,
        (*info).Context as HANDLE,
    );
    post_op(&call, status)
}

#[no_mangle]
pub unsafe extern "stdcall" fn MirrorSetFileAttributes(
    path: LPCWSTR,
    attributes: DWORD,
    _: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let real_path = trans_ppath!(path);
    let call = VFSCall::chmod(chmod {
        path: Cow::Owned(wstr_to_path(path)),
        mode: attributes,
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = OpSetFileAttributes(real_path.as_ptr(), attributes);
    post_op(&call, status)
}
pub unsafe extern "stdcall" fn MirrorSetFileTime(
    path: LPCWSTR,
    creation: *const FILETIME,
    access: *const FILETIME,
    write: *const FILETIME,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let real_path = trans_ppath!(path);
    let call = VFSCall::utimens(utimens {
        path: Cow::Owned(wstr_to_path(path)),
        timespec: [
            enc_timespec::from(*creation),
            enc_timespec::from(*access),
            enc_timespec::from(*write),
        ],
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = OpSetFileTime(
        real_path.as_ptr(),
        creation,
        access,
        write,
        (*info).Context as HANDLE,
    );
    post_op(&call, status)
}

#[no_mangle]
pub unsafe extern "stdcall" fn MirrorMoveFile(
    path: LPCWSTR,
    new_name: LPCWSTR,
    replace: BOOL,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let real_path = trans_ppath!(path);
    let real_new_name = trans_ppath!(new_name);
    let call = VFSCall::rename(rename {
        from: Cow::Owned(wstr_to_path(path)),
        to: Cow::Owned(wstr_to_path(new_name)),
        flags: replace as uint32_t,
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = OpMoveFile(
        real_path.as_ptr(),
        real_new_name.as_ptr(),
        replace,
        (*info).Context as HANDLE,
    );
    post_op(&call, status)
}

#[no_mangle]
pub unsafe extern "stdcall" fn MirrorSetEndOfFile(
    path: LPCWSTR,
    offset: LONGLONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let real_path = trans_ppath!(path);
    let call = VFSCall::truncate(truncate {
        path: Cow::Owned(wstr_to_path(path)),
        size: offset,
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = OpSetEndOfFile(real_path.as_ptr(), offset, (*info).Context as HANDLE);
    post_op(&call, status)
}

#[no_mangle]
pub unsafe extern "stdcall" fn MirrorSetAllocationSize(
    path: LPCWSTR,
    size: LONGLONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let real_path = trans_ppath!(path);
    let call = VFSCall::allocation_size(allocation_size {
        path: Cow::Owned(wstr_to_path(path)),
        size,
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = OpSetAllocationSize(real_path.as_ptr(), size, (*info).Context as HANDLE);
    post_op(&call, status)
}

fn lookup_account(psid: PSID) -> Option<(String, String)> {
    // TODO keep cache of lookups
    use std::ffi::CStr;
    use winapi::shared::winerror::ERROR_NONE_MAPPED;
    use winapi::um::errhandlingapi::GetLastError;
    use winapi::um::winbase::LookupAccountSidA;
    use winapi::um::winnt::SID_NAME_USE;

    let mut name_buf: [u8; 256] = [0; 256];
    let mut name_len = name_buf.len() as u32;
    let mut domain_buf: [u8; 256] = [0; 256];
    let mut domain_len = domain_buf.len() as u32;
    let mut acc_type: SID_NAME_USE = 0;
    if LookupAccountSidA(
        ptr::null(),
        psid,
        name_buf.as_ptr() as *mut _,
        &mut name_len as *mut _,
        domain_buf.as_ptr() as *mut _,
        &mut domain_len as *mut _,
        &mut acc_type as *mut _,
    ) == 0
    {
        let err = GetLastError();
        println!("Accont loookup error occured");
        match err {
            ERROR_NONE_MAPPED => return None,
            _ => return None,
        }
    }
    let domain = String::from(
        CStr::from_bytes_with_nul(&domain_buf[..domain_len as usize])
            .expect("Domain is an invalid CString")
            .to_str()
            .expect("Domain cannot be represented in UTF-8"),
    );
    let name = String::from(
        CStr::from_bytes_with_nul(&name_buf[..name_len as usize])
            .expect("Account name is an invalid CString")
            .to_str()
            .expect("Account name be represented in UTF-8"),
    );
    return Some((domain, name));
}

#[no_mangle]
pub unsafe extern "stdcall" fn MirrorSetFileSecurity(
    path: LPCWSTR,
    security: PSECURITY_INFORMATION,
    descriptor: PSECURITY_DESCRIPTOR,
    length: ULONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let real_path = trans_ppath!(path);
    use winapi::um::winnt::{
        DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
        SACL_SECURITY_INFORMATION,
    };

    let desc = descriptor as *const SECURITY_DESCRIPTOR;

    if flagset!(*security, SACL_SECURITY_INFORMATION) {
        (*desc).Sacl;
        panic!("Sacl replication not implemented");
    }

    let fileSecurity = FileSecurity::Windows {
        dacl: if flagset!(*security, DACL_SECURITY_INFORMATION) {
            (*desc).Dacl;
            panic!("Dacl replication not implemented");
        } else {
            None
        },
        group: if flagset!(*security, GROUP_SECURITY_INFORMATION) {
            let (_, acc_name) = lookup_account((*desc).Group).expect("Unknown account");
            Some(acc_name)
        } else {
            None
        },
        owner: if flagset!(*security, OWNER_SECURITY_INFORMATION) {
            let (_, acc_name) = lookup_account((*desc).Owner).expect("Unknown account");
            Some(acc_name)
        } else {
            None
        },
    };

    let call = VFSCall::security(security {
        path: Cow::Owned(wstr_to_path(path)),
        security: fileSecurity,
    });

    if let Some(r) = pre_op(&call) {
        return r;
    }

    let status = OpSetFileSecurity(
        real_path.as_ptr(),
        security,
        descriptor,
        (*info).Context as HANDLE,
    );
    post_op(&call, status)
}

#[no_mangle]
pub unsafe extern "stdcall" fn MirrorFlushFileBuffers(
    path: LPCWSTR,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let real_path = trans_ppath!(path);
    let call = VFSCall::fsync(fsync {
        path: Cow::Owned(wstr_to_path(path)),
        isdatasync: 0,
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = OpFlushFileBuffers(real_path.as_ptr(), (*info).Context as HANDLE);
    post_op(&call, status)
}
