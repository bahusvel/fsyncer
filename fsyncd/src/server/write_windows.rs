use common::*;
use libc::uint32_t;
use server::{dokan::*, post_op, pre_op, SERVER_PATH};
use std::borrow::Cow;
use std::fs::symlink_metadata;
use std::io::{Error, ErrorKind};
use std::ptr;
use std::slice;
use winapi::um::fileapi::*;
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
use winapi::um::winnt::{FILE_SHARE_READ, PACL, PSID, SECURITY_DESCRIPTOR};

unsafe fn as_user<O, F: (FnOnce() -> O)>(handle: HANDLE, f: F) -> O {
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
    use winapi::um::winbase::FILE_FLAG_BACKUP_SEMANTICS;

    debug!(
        wstr_to_path(path),
        context, access, attributes, shared, disposition, options, info
    );

    let rpath = wstr_to_path(path);
    let rrpath = translate_path(&rpath, SERVER_PATH.as_ref().unwrap());
    let real_path = path_to_wstr(&rrpath);

    let attr = symlink_metadata(rrpath);
    let exists = !(attr.is_err() && attr.as_ref().unwrap_err().kind() == ErrorKind::NotFound);
    // File exists and we need to open it

    let mut user_access = 0 as ACCESS_MASK;
    let mut user_attributes = 0 as DWORD;
    let mut user_disposition = 0 as DWORD;
    let sec_desc = (*context).AccessState.SecurityDescriptor;

    let user_handle = DokanOpenRequestorToken(info);
    assert!(user_handle != INVALID_HANDLE_VALUE);

    DokanMapKernelToUserCreateFileFlags(
        access,
        attributes,
        options,
        disposition,
        &mut user_access as *mut _,
        &mut user_attributes as *mut _,
        &mut user_disposition as *mut _,
    );

    if exists && attr.unwrap().is_dir() {
        if !flagset!(options, FILE_NON_DIRECTORY_FILE) {
            (*info).IsDirectory = 1;
            shared |= FILE_SHARE_READ;
            user_attributes |= FILE_FLAG_BACKUP_SEMANTICS;
        } else {
            return STATUS_FILE_IS_A_DIRECTORY;
        }
    }

    let mut call = None;
    let status;
    let desc = sec_desc as *const SECURITY_DESCRIPTOR;

    let security = if desc.is_null() {
        // TODO if descriptor is NULL default descriptor is assigned, I probably need to query it and send it to the other side
        FileSecurity::Windows {
            owner: None,
            group: None,
            dacl: None,
            sacl: None,
        }
    } else {
        FileSecurity::Windows {
            // This is invalid, I tthink I need to use GetSecurityDescriptorXXXX
            owner: {
                if (*desc).Owner.is_null() {
                    None
                } else {
                    let (_, acc_name) = lookup_account((*desc).Owner).expect("Unknown account");
                    Some(acc_name)
                }
            },
            group: {
                if (*desc).Group.is_null() {
                    None
                } else {
                    let (_, acc_name) = lookup_account((*desc).Group).expect("Unknown account");
                    Some(acc_name)
                }
            },
            dacl: if (*desc).Dacl.is_null() {
                None
            } else {
                Some(
                    file_security::acl_entries((*desc).Dacl).expect("Failed to parse DACL entries"),
                )
            },
            sacl: if (*desc).Sacl.is_null() {
                None
            } else {
                Some(
                    file_security::acl_entries((*desc).Sacl).expect("Failed to parse SACL entries"),
                )
            },
        }
    };

    if !exists
        && (flagset!(disposition, CREATE_ALWAYS) || flagset!(disposition, CREATE_NEW))
        && (*info).IsDirectory != 0
    {
        // Path does not exist, need to create it, and it is a directory
        call = Some(VFSCall::mkdir(mkdir {
            path: Cow::Borrowed(&rpath),
            security,
            mode: user_attributes,
        }));
        if let Some(r) = pre_op(call.as_ref().unwrap()) {
            return r;
        }
        println!("Trying to create folder");
        status = as_user(user_handle, || {
            OpCreateDirectory(
                real_path.as_ptr(),
                sec_desc,
                user_access,
                user_attributes,
                shared,
                user_disposition,
                &mut (*info).Context as *mut _ as *mut _,
            )
        });
    } else {
        if exists
            && (flagset!(disposition, TRUNCATE_EXISTING) || flagset!(disposition, CREATE_ALWAYS))
        {
            call = Some(VFSCall::truncate(truncate {
                path: Cow::Borrowed(&rpath),
                size: 0,
            }));
        } else if !exists
            && (flagset!(disposition, CREATE_ALWAYS) || flagset!(disposition, CREATE_NEW))
        {
            call = Some(VFSCall::create(create {
                path: Cow::Borrowed(&rpath),
                security,
                mode: user_attributes,
                flags: user_disposition as i32,
            }));
        }

        if call.is_some() {
            if let Some(r) = pre_op(call.as_ref().unwrap()) {
                return r;
            }
        }

        status = as_user(user_handle, || {
            OpCreateFile(
                real_path.as_ptr(),
                sec_desc,
                user_access,
                user_attributes,
                shared,
                user_disposition,
                &mut (*info).Context as *mut _ as *mut _,
            )
        });
    }

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

    if (*info).DeleteOnClose == 0 {
        // Don't need to delete the file
        return;
    }

    let call;
    let status;

    if (*info).IsDirectory != 0 {
        call = VFSCall::rmdir(rmdir {
            path: Cow::Owned(wstr_to_path(path)),
        });
        if let Some(_) = pre_op(&call) {
            return;
        }
        status = OpDeleteDirectory(real_path.as_ptr());
    } else {
        call = VFSCall::unlink(unlink {
            path: Cow::Owned(wstr_to_path(path)),
        });
        if let Some(_) = pre_op(&call) {
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
    mut len: DWORD,
    bytes_written: LPDWORD,
    mut offset: LONGLONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let rpath = wstr_to_path(path);
    let rrpath = translate_path(&rpath, SERVER_PATH.as_ref().unwrap());
    let real_path = path_to_wstr(&rrpath);

    let stat = symlink_metadata(rrpath);
    if stat.is_err() {
        return DokanNtStatusFromWin32(GetLastError());
    }
    let file_size = stat.unwrap().len();

    if (*info).WriteToEndOfFile != 0 {
        offset = file_size as i64;
    } else {
        // Paging IO cannot write after allocate file size.
        if (*info).PagingIo != 0 {
            if offset as u64 >= file_size {
                *bytes_written = 0;
                return STATUS_SUCCESS;
            }
            if (offset as u64 + len as u64) > file_size {
                let bytes = file_size - offset as u64;
                if (bytes >> 32) != 0 {
                    len = (bytes & 0xFFFFFFFF) as u32;
                } else {
                    len = bytes as u32;
                }
            }
        }
        if offset as u64 > file_size {
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

#[no_mangle]
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

unsafe fn lookup_account(psid: PSID) -> Result<(String, String), Error> {
    // TODO keep cache of lookups
    use std::ffi::CStr;
    use winapi::um::winbase::LookupAccountSidA;
    use winapi::um::winnt::SID_NAME_USE;

    let name_buf: [u8; 256] = [0; 256];
    let mut name_len = name_buf.len() as u32;
    let domain_buf: [u8; 256] = [0; 256];
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
        return Err(Error::last_os_error());
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
    return Ok((domain, name));
}

unsafe fn acl_from_descriptor(
    descriptor: PSECURITY_DESCRIPTOR,
    dacl: bool,
) -> Result<(PACL, bool), Error> {
    use winapi::um::securitybaseapi::{GetSecurityDescriptorDacl, GetSecurityDescriptorSacl};
    let mut ppacl = ptr::null_mut();
    let mut present = 0;
    let mut defaulted = 0;

    if dacl {
        if GetSecurityDescriptorDacl(
            descriptor,
            &mut present as *mut _,
            &mut ppacl as *mut _,
            &mut defaulted as *mut _,
        ) == 0
        {
            return Err(Error::last_os_error());
        }
    } else {
        if GetSecurityDescriptorSacl(
            descriptor,
            &mut present as *mut _,
            &mut ppacl as *mut _,
            &mut defaulted as *mut _,
        ) == 0
        {
            return Err(Error::last_os_error());
        }
    }

    if present == 0 {
        debug!(dacl);
        ppacl = ptr::null_mut();
    }

    Ok((ppacl, defaulted != 0))
}

#[no_mangle]
pub unsafe extern "stdcall" fn MirrorSetFileSecurity(
    path: LPCWSTR,
    security: PSECURITY_INFORMATION,
    descriptor: PSECURITY_DESCRIPTOR,
    _length: ULONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let real_path = trans_ppath!(path);
    use winapi::um::winnt::{
        DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
        SACL_SECURITY_INFORMATION,
    };

    let desc = descriptor as *const SECURITY_DESCRIPTOR;

    let file_sec = FileSecurity::Windows {
        dacl: if flagset!(*security, DACL_SECURITY_INFORMATION) {
            let (dacl, _) =
                acl_from_descriptor(desc as *mut _, true).expect("Failed to parse DACL entries");
            if dacl.is_null() {
                None
            } else {
                Some(file_security::acl_entries(dacl).expect("Failed to parse DACL entries"))
            }
        } else {
            None
        },
        sacl: if flagset!(*security, SACL_SECURITY_INFORMATION) {
            let (sacl, _) =
                acl_from_descriptor(desc as *mut _, false).expect("Failed to parse SACL entries");
            if sacl.is_null() {
                None
            } else {
                Some(file_security::acl_entries(sacl).expect("Failed to parse SACL entries"))
            }
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
        security: file_sec,
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
