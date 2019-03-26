use common::*;
use libc::uint32_t;
use server::{dokan::*, post_op, pre_op, SERVER_PATH};
use std::borrow::Cow;
use std::fs::symlink_metadata;
use std::io::ErrorKind;
use std::mem;
use std::slice;
use winapi::um::fileapi::*;
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
use winapi::um::winnt::FILE_SHARE_READ;

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
    dokan_access: ACCESS_MASK,
    dokan_attributes: ULONG,
    mut shared: ULONG,
    dokan_disposition: ULONG,
    dokan_options: ULONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    debug!(wstr_to_path(path));
    use winapi::shared::ntstatus::STATUS_FILE_IS_A_DIRECTORY;
    use winapi::um::winbase::FILE_FLAG_BACKUP_SEMANTICS;

    let rpath = wstr_to_path(path);
    let rrpath = translate_path(&rpath, SERVER_PATH.as_ref().unwrap());
    let real_path = path_to_wstr(&rrpath);
    let attr = symlink_metadata(rrpath);

    if attr.is_err() && attr.as_ref().unwrap_err().kind() != ErrorKind::NotFound
    {
        //This is introduced because cmd.exe will try to open paths like
        // \\<partial>*. This is exactly what mirror.c from Dokan does, it
        // doesn't handle them properly, just errors with 123 code.
        return DokanNtStatusFromWin32(
            attr.unwrap_err()
                .raw_os_error()
                .expect("Failed to extract OS error") as u32,
        );
    }

    let exists = !(attr.is_err()
        && attr.as_ref().unwrap_err().kind() == ErrorKind::NotFound);
    // File exists and we need to open it

    let mut user_access = 0 as ACCESS_MASK;
    let mut user_attributes = 0 as DWORD;
    let mut user_disposition = 0 as DWORD;
    let sec_desc = (*context).AccessState.SecurityDescriptor;

    let user_handle = DokanOpenRequestorToken(info);
    assert!(user_handle != INVALID_HANDLE_VALUE);

    DokanMapKernelToUserCreateFileFlags(
        dokan_access,
        dokan_attributes,
        dokan_options,
        dokan_disposition,
        &mut user_access as *mut _,
        &mut user_attributes as *mut _,
        &mut user_disposition as *mut _,
    );

    //debug!("Create file", wstr_to_path(path), user_disposition);

    if exists && attr.as_ref().unwrap().is_dir() {
        if !flagset!(dokan_options, FILE_NON_DIRECTORY_FILE) {
            (*info).IsDirectory = 1;
            shared |= FILE_SHARE_READ;
            user_attributes |= FILE_FLAG_BACKUP_SEMANTICS;
        } else {
            return STATUS_FILE_IS_A_DIRECTORY;
        }
    }

    let mut call = None;
    let status;

    let security = if sec_desc.is_null() {
        // TODO if descriptor is NULL default descriptor is assigned, I probably
        // need to query it and send it to the other side
        FileSecurity::Default
    } else {
        let s = FileSecurity::parse_security(sec_desc, None, false)
            .expect("Failed to parse security descriptor");
        debug!(s);
        s
    };

    if !exists
        && (user_disposition == CREATE_ALWAYS || user_disposition == CREATE_NEW)
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
        use std::os::windows::fs::MetadataExt;
        use winapi::shared::ntstatus::STATUS_CANNOT_DELETE;
        use winapi::um::winbase::FILE_FLAG_DELETE_ON_CLOSE;
        use winapi::um::winnt::FILE_ATTRIBUTE_READONLY;
        if (exists
            && flagset!(
                attr.as_ref().unwrap().file_attributes(),
                FILE_ATTRIBUTE_READONLY
            )
            || flagset!(user_attributes, FILE_ATTRIBUTE_READONLY))
            && flagset!(user_attributes, FILE_FLAG_DELETE_ON_CLOSE)
        {
            return STATUS_CANNOT_DELETE;
        }

        //debug!(exists);

        if exists
            && (user_disposition == TRUNCATE_EXISTING
                || user_disposition == CREATE_ALWAYS)
        {
            call = Some(VFSCall::truncate(truncate {
                path: Cow::Borrowed(&rpath),
                size: 0,
            }));
        } else if !exists
            && (user_disposition == CREATE_ALWAYS
                || user_disposition == CREATE_NEW
                || user_disposition == OPEN_ALWAYS)
        // Should also check if it can create (i.e. GENERIC_WRITE)
        /*
        In case of CREATE_ALWAYS, shouldn't I always replicate it? One would
        imagine it would delete the old file and create a new one in its
        place. But CreateFile from MSDN suggests it just overwrites it... so
        does the new file get created or simply the data gets truncated?
        */
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
                return DokanNtStatusFromWin32(r as u32);
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
        let mut s = DokanNtStatusFromWin32(post_op(
            call.as_ref().unwrap(),
            status as i32,
        ) as u32);
        if s == STATUS_SUCCESS
            && (*info).Context as *mut _ != INVALID_HANDLE_VALUE
            && user_disposition == OPEN_ALWAYS
            && exists
        {
            use winapi::shared::ntstatus::STATUS_OBJECT_NAME_COLLISION;
            // Open succeed but we need to inform the driver
            // that the dir open and not created by returning
            // STATUS_OBJECT_NAME_COLLISION
            s = STATUS_OBJECT_NAME_COLLISION;
        }
        return s;
    } else {
        return DokanNtStatusFromWin32(status);
    }
}

#[no_mangle]
pub unsafe extern "stdcall" fn MirrorCleanup(
    path: LPCWSTR,
    info: PDOKAN_FILE_INFO,
) {
    let real_path = trans_ppath!(path);
    let handle = (*info).Context as HANDLE;
    if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
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
    post_op(&call, status as i32);
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

    if (*info).WriteToEndOfFile != 0 {
        offset = std::i64::MAX;
    } else if (*info).PagingIo != 0 {
        println!("Write path hit \"stat\"");
        let rrpath = translate_path(&rpath, SERVER_PATH.as_ref().unwrap());
        let stat = symlink_metadata(rrpath); // FIXME, I must avoid "stat" like wild fire in this code path!
        if stat.is_err() {
            return DokanNtStatusFromWin32(
                stat.unwrap_err()
                    .raw_os_error()
                    .expect("Failed to get OS error") as u32,
            );
        }
        let file_size = stat.unwrap().len();

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

    let call = VFSCall::write(write {
        path: Cow::Owned(rpath),
        buf: Cow::Borrowed(slice::from_raw_parts(
            buffer as *const u8,
            len as usize,
        )),
        offset,
    });
    if let Some(r) = pre_op(&call) {
        return DokanNtStatusFromWin32(r as u32);
    }
    let status = OpWriteFile(
        buffer,
        len,
        bytes_written,
        offset,
        (*info).Context as HANDLE,
    );

    DokanNtStatusFromWin32(post_op(&call, status as i32) as u32)
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
        return DokanNtStatusFromWin32(r as u32);
    }
    let status = OpSetFileAttributes(real_path.as_ptr(), attributes);
    DokanNtStatusFromWin32(post_op(&call, status as i32) as u32)
}

#[no_mangle]
pub unsafe extern "stdcall" fn MirrorSetFileTime(
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
        return DokanNtStatusFromWin32(r as u32);
    }
    let status =
        OpSetFileTime(creation, access, write, (*info).Context as HANDLE);
    DokanNtStatusFromWin32(post_op(&call, status as i32) as u32)
}

#[no_mangle]
pub unsafe extern "stdcall" fn MirrorMoveFile(
    path: LPCWSTR,
    new_name: LPCWSTR,
    replace: BOOL,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let real_new_name = trans_ppath!(new_name);
    let call = VFSCall::rename(rename {
        from: Cow::Owned(wstr_to_path(path)),
        to: Cow::Owned(wstr_to_path(new_name)),
        flags: replace as uint32_t,
    });
    if let Some(r) = pre_op(&call) {
        return DokanNtStatusFromWin32(r as u32);
    }
    let status =
        OpMoveFile(real_new_name.as_ptr(), replace, (*info).Context as HANDLE);
    DokanNtStatusFromWin32(post_op(&call, status as i32) as u32)
}

#[no_mangle]
pub unsafe extern "stdcall" fn MirrorSetEndOfFile(
    path: LPCWSTR,
    offset: LONGLONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let call = VFSCall::truncate(truncate {
        path: Cow::Owned(wstr_to_path(path)),
        size: offset,
    });
    if let Some(r) = pre_op(&call) {
        return DokanNtStatusFromWin32(r as u32);
    }
    let status = OpSetEndOfFile(offset, (*info).Context as HANDLE);
    DokanNtStatusFromWin32(post_op(&call, status as i32) as u32)
}

#[no_mangle]
pub unsafe extern "stdcall" fn MirrorSetAllocationSize(
    path: LPCWSTR,
    size: LONGLONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    /*
    https://docs.microsoft.com/en-gb/windows/desktop/api/winbase/ns-winbase-_file_allocation_info

    The end-of-file (EOF) position for a file must always be less than or
    equal to the file allocation size. If the allocation size is set to a
    value that is less than EOF, the EOF position is automatically adjusted
    to match the file allocation size.

    This is the behaviour dokan is trying to emulate. But doesn't actually
    adjust file allocation size to be larger, via SetInformationByHandle
    call. I'm not entirely sure if this is correct behaviour. From the
    replication perspective if AllocationSize can be queried it could be
    different. Althought I think it shouldn't be.
    */

    use winapi::um::fileapi::GetFileSizeEx;
    use winapi::um::winnt::LARGE_INTEGER;

    let mut file_size: LARGE_INTEGER = mem::zeroed();

    if GetFileSizeEx((*info).Context as HANDLE, &mut file_size) == 0 {
        return DokanNtStatusFromWin32(GetLastError());
    }

    if size >= *file_size.QuadPart() {
        return STATUS_SUCCESS;
    }

    let call = VFSCall::truncate(truncate {
        path: Cow::Owned(wstr_to_path(path)),
        size,
    });
    if let Some(r) = pre_op(&call) {
        return DokanNtStatusFromWin32(r as u32);
    }
    let status = OpSetEndOfFile(size, (*info).Context as HANDLE);
    DokanNtStatusFromWin32(post_op(&call, status as i32) as u32)
}

#[no_mangle]
pub unsafe extern "stdcall" fn MirrorSetFileSecurity(
    path: LPCWSTR,
    security: PSECURITY_INFORMATION,
    descriptor: PSECURITY_DESCRIPTOR,
    _length: ULONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let file_sec =
        FileSecurity::parse_security(descriptor, Some(security), false)
            .expect("Failed to parse security");

    println!("Security {:#?}", file_sec);

    let call = VFSCall::security(security {
        path: Cow::Owned(wstr_to_path(path)),
        security: file_sec,
    });

    if let Some(r) = pre_op(&call) {
        return DokanNtStatusFromWin32(r as u32);
    }

    let status =
        OpSetFileSecurity(security, descriptor, (*info).Context as HANDLE);
    DokanNtStatusFromWin32(post_op(&call, status as i32) as u32)
}

#[no_mangle]
pub unsafe extern "stdcall" fn MirrorFlushFileBuffers(
    path: LPCWSTR,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let call = VFSCall::fsync(fsync {
        path: Cow::Owned(wstr_to_path(path)),
        isdatasync: 0,
    });
    if let Some(r) = pre_op(&call) {
        return DokanNtStatusFromWin32(r as u32);
    }
    let status = OpFlushFileBuffers((*info).Context as HANDLE);
    DokanNtStatusFromWin32(post_op(&call, status as i32) as u32)
}
