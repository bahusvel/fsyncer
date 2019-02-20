extern crate dokan;

use self::dokan::*;
use common::*;
use libc::{uint32_t, wcslen};
use server::{post_op, pre_op};
use std::borrow::Cow;
use std::fs::symlink_metadata;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::ptr;
use std::slice;
use winapi::um::fileapi::*;
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
use winapi::um::winnt::{FILE_SHARE_READ, PSID};

fn wstr_to_path<'a>(path: LPCWSTR) -> PathBuf {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    let len = wcslen(path);
    PathBuf::from(OsString::from_wide(slice::from_raw_parts(path, len)))
}

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

pub unsafe extern "stdcall" fn zw_create_file(
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
    let attr = symlink_metadata(rpath);
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

    if !exists
        && (flagset!(disposition, CREATE_ALWAYS) || flagset!(disposition, CREATE_NEW))
        && (*info).IsDirectory != 0
    {
        // Path does not exist, need to create it, and it is a directory
        call = Some(VFSCall::mkdir(mkdir {
            path: Cow::Borrowed(&rpath),
            uid: 0,
            gid: 0,
            mode: 0,
        }));
        if let Some(r) = pre_op(call.as_ref().unwrap()) {
            return r;
        }
        status = as_user(userHandle, || {
            MirrorCreateDirectory(
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

    if exists && (flagset!(disposition, TRUNCATE_EXISTING)) {
        call = Some(VFSCall::truncate(truncate {
            path: Cow::Borrowed(&rpath),
            size: 0,
        }));
    } else if !exists && (flagset!(disposition, CREATE_ALWAYS) || flagset!(disposition, CREATE_NEW))
    {
        call = Some(VFSCall::create(create {
            path: Cow::Borrowed(&rpath),
            uid: 0,
            gid: 0,
            mode: 0,
            flags: 0,
        }));
    }

    if call.is_some() {
        if let Some(r) = pre_op(call.as_ref().unwrap()) {
            return r;
        }
    }
    status = as_user(userHandle, || {
        MirrorCreateFile(
            path,
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
pub unsafe extern "stdcall" fn cleanup(path: LPCWSTR, info: PDOKAN_FILE_INFO) {}
pub unsafe extern "stdcall" fn write_file(
    path: LPCWSTR,
    buffer: LPCVOID,
    len: DWORD,
    bytes_written: LPDWORD,
    offset: LONGLONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let call = VFSCall::write(write {
        path: Cow::Owned(wstr_to_path(path)),
        buf: Cow::Borrowed(slice::from_raw_parts(buffer as *const u8, len as usize)),
        offset,
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = MirrorWriteFile(
        path,
        buffer,
        len,
        bytes_written,
        offset,
        (*info).Context as HANDLE,
    );
    post_op(&call, status)
}
pub unsafe extern "stdcall" fn set_file_attributes(
    path: LPCWSTR,
    attributes: DWORD,
    _: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let call = VFSCall::chmod(chmod {
        path: Cow::Owned(wstr_to_path(path)),
        mode: attributes,
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = MirrorSetFileAttributes(path, attributes);
    post_op(&call, status)
}
pub unsafe extern "stdcall" fn set_file_time(
    path: LPCWSTR,
    creation: *const FILETIME,
    access: *const FILETIME,
    write: *const FILETIME,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
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
    let status = MirrorSetFileTime(path, creation, access, write, (*info).Context as HANDLE);
    post_op(&call, status)
}
pub unsafe extern "stdcall" fn move_file(
    path: LPCWSTR,
    new_name: LPCWSTR,
    replace: BOOL,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let call = VFSCall::rename(rename {
        from: Cow::Owned(wstr_to_path(path)),
        to: Cow::Owned(wstr_to_path(new_name)),
        flags: replace as uint32_t,
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = MirrorMoveFile(path, new_name, replace, (*info).Context as HANDLE);
    post_op(&call, status)
}
pub unsafe extern "stdcall" fn set_end_of_file(
    path: LPCWSTR,
    offset: LONGLONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let call = VFSCall::truncate(truncate {
        path: Cow::Owned(wstr_to_path(path)),
        size: offset,
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = MirrorSetEndOfFile(path, offset, (*info).Context as HANDLE);
    post_op(&call, status)
}
pub unsafe extern "stdcall" fn set_allocation_size(
    path: LPCWSTR,
    size: LONGLONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let call = VFSCall::allocation_size(allocation_size {
        path: Cow::Owned(wstr_to_path(path)),
        size,
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = MirrorSetAllocationSize(path, size, (*info).Context as HANDLE);
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

pub unsafe extern "stdcall" fn set_file_security(
    path: LPCWSTR,
    security: PSECURITY_INFORMATION,
    descriptor: PSECURITY_DESCRIPTOR,
    length: ULONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    use winapi::um::winnt::{
        DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
        SACL_SECURITY_INFORMATION, SECURITY_DESCRIPTOR,
    };

    let desc = descriptor as *const SECURITY_DESCRIPTOR;

    if flagset!(*security, SACL_SECURITY_INFORMATION) {
        (*desc).Sacl;
        panic!("Sacl replication not implemented");
    }

    let call = VFSCall::security(security {
        path: Cow::Owned(wstr_to_path(path)),
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
    });

    if let Some(r) = pre_op(&call) {
        return r;
    }

    let status = MirrorSetFileSecurity(path, security, descriptor, (*info).Context as HANDLE);
    post_op(&call, status)
}
pub unsafe extern "stdcall" fn flush_file_buffers(
    path: LPCWSTR,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let call = VFSCall::fsync(fsync {
        path: Cow::Owned(wstr_to_path(path)),
        isdatasync: 0,
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = MirrorFlushFileBuffers(path, (*info).Context as HANDLE);
    post_op(&call, status)
}
