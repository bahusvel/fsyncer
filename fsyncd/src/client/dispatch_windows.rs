use common::*;
use libc::c_int;
use std::fs::OpenOptions;
use std::path::Path;

fn with_file<F: (FnOnce(HANDLE) -> DWORD)>(path: &Path, options: &OpenOptions, f: F) -> DWORD {
    use std::os::windows::io::IntoRawHandle;
    let file = options.open(path);
    if file.is_err() {
        return winapi::shared::winerror::ERROR_INVALID_HANDLE;
    }
    let handle = file.unwrap().into_raw_handle();
    let res = f(handle);
    unsafe { winapi::um::handleapi::CloseHandle(handle) };
    res
}

pub unsafe fn dispatch(call: &VFSCall, root: &Path) -> c_int {
    use std::ptr;
    use winapi::um::fileapi::CREATE_NEW;
    use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
    use winapi::um::winnt::{
        DACL_SECURITY_INFORMATION, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        GENERIC_WRITE, GROUP_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
        SACL_SECURITY_INFORMATION, SECURITY_DESCRIPTOR,
    };
    match call {
        VFSCall::utimens(utimens { path, timespec }) => {
            let created = timespec[0].clone().into();
            let accessed = timespec[1].clone().into();
            let written = timespec[2].clone().into();
            with_file(
                &translate_path(path, root),
                OpenOptions::new().write(true),
                |handle| {
                    OpSetFileTime(
                        &created as *const FILETIME,
                        &accessed as *const FILETIME,
                        &written as *const FILETIME,
                        handle,
                    )
                },
            ) as i32
        }

        VFSCall::create(create {
            path,
            mode,
            flags,
            security,
        }) => {
            let rpath = translate_path(path, root);
            let real_path = path_to_wstr(&rpath);
            //let mut descriptor = mem::zeroed(); // = security.to_descriptor();
            let mut handle = INVALID_HANDLE_VALUE;
            // Giving it loosest sharing access may not be a good idea, I may need to replicate.
            let res = OpCreateFile(
                real_path.as_ptr(),
                ptr::null_mut(), // FIXME
                GENERIC_WRITE,
                FILE_SHARE_DELETE | FILE_SHARE_READ | FILE_SHARE_WRITE,
                *mode,         // attributes
                *flags as u32, // disposition
                &mut handle as *mut _,
            );
            if handle != INVALID_HANDLE_VALUE {
                CloseHandle(handle);
            }
            res as i32
        }
        VFSCall::write(write { path, buf, offset }) => {
            let mut bytes_written: u32 = 0;
            with_file(
                &translate_path(path, root),
                OpenOptions::new().write(true),
                |handle| {
                    OpWriteFile(
                        buf.as_ptr() as *const _,
                        buf.len() as u32,
                        &mut bytes_written as *mut _,
                        *offset,
                        handle,
                    )
                },
            ) as i32
        }
        VFSCall::truncate(truncate { path, size }) => with_file(
            &translate_path(path, root),
            OpenOptions::new().write(true),
            |handle| OpSetEndOfFile(*size, handle),
        ) as i32,
        VFSCall::rename(rename { from, to, flags }) => {
            let rto = translate_path(to, root);
            let real_to = path_to_wstr(&rto);
            with_file(
                &translate_path(from, root),
                OpenOptions::new().write(true),
                |handle| OpMoveFile(real_to.as_ptr(), *flags as i32, handle),
            ) as i32
        }
        VFSCall::rmdir(rmdir { path }) => {
            let rpath = translate_path(path, root);
            let real_path = path_to_wstr(&rpath);
            OpDeleteDirectory(real_path.as_ptr()) as i32
        }
        VFSCall::unlink(unlink { path }) => {
            let rpath = translate_path(path, root);
            let real_path = path_to_wstr(&rpath);
            OpDeleteFile(real_path.as_ptr()) as i32
        }
        VFSCall::mkdir(mkdir {
            path,
            mode,
            security,
        }) => {
            let rpath = translate_path(path, root);
            let real_path = path_to_wstr(&rpath);
            //let mut descriptor = mem::zeroed(); // = security.to_descriptor();
            let mut handle = INVALID_HANDLE_VALUE;
            // Giving it loosest sharing access may not be a good idea, I may need to replicate.
            let res = OpCreateDirectory(
                real_path.as_ptr(),
                ptr::null_mut(), // FIXME
                GENERIC_WRITE,
                FILE_SHARE_DELETE | FILE_SHARE_READ | FILE_SHARE_WRITE,
                *mode,      // attributes
                CREATE_NEW, // disposition
                &mut handle as *mut _,
            );
            if handle != INVALID_HANDLE_VALUE {
                CloseHandle(handle);
            }
            res as i32
        }
        VFSCall::security(security { path, security }) => {
            let mut info = 0;

            if let FileSecurity::Windows {
                owner,
                group,
                dacl,
                sacl,
            } = security
            {
                if owner.is_some() {
                    info |= OWNER_SECURITY_INFORMATION;
                }
                if group.is_some() {
                    info |= GROUP_SECURITY_INFORMATION;
                }
                if dacl.is_some() {
                    info |= DACL_SECURITY_INFORMATION;
                }
                if sacl.is_some() {
                    info |= SACL_SECURITY_INFORMATION;
                }
            } else {
                panic!("Security information needs translation")
            }

            //let mut descriptor = mem::zeroed(); // = security.to_descriptor();
            with_file(
                &translate_path(path, root),
                OpenOptions::new().write(true),
                |handle| {
                    OpSetFileSecurity(
                        &mut info as *mut _,
                        ptr::null_mut(), // FIXME
                        handle,
                    )
                },
            ) as i32
        }
        VFSCall::fsync(_) => ERROR_SUCCESS as i32, // Don't need to execute it, just needed for flush synchronous mode
        _ => panic!("Windows cannot dispatch {:?}, translation required", call),
    }
}
