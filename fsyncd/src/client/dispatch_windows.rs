use common::FileSecurity;
use common::*;
use libc::c_int;
use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;
use std::ptr;

trait ErrorOrOk<T> {
    fn err_or_ok(self) -> T;
}

impl<T> ErrorOrOk<T> for Result<T, T> {
    fn err_or_ok(self) -> T {
        match self {
            Err(t) => t,
            Ok(t) => t,
        }
    }
}

pub unsafe fn dispatch(call: &VFSCall, root: &Path) -> c_int {
    use winapi::um::fileapi::CREATE_NEW;
    use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
    use winapi::um::winnt::{
        DACL_SECURITY_INFORMATION, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, GENERIC_WRITE, OWNER_SECURITY_INFORMATION,
        UNPROTECTED_DACL_SECURITY_INFORMATION, WRITE_DAC, WRITE_OWNER,
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
                    ) as i32
                },
            )
            .map_err(|e| e.raw_os_error().unwrap())
            .err_or_ok()
        }

        VFSCall::create(create {
            path,
            mode,
            flags,
            security,
        }) => {
            let rpath = translate_path(path, root);
            let real_path = path_to_wstr(&rpath);
            let descriptor = security
                .clone()
                .to_descriptor()
                .expect("Failed to create security descriptor");

            let mut handle = INVALID_HANDLE_VALUE;
            // Giving it loosest sharing access may not be a good idea, I may
            // need to replicate.
            let mut res = OpCreateFile(
                real_path.as_ptr(),
                descriptor
                    .map(|d| d.as_ptr() as *mut _)
                    .unwrap_or(ptr::null_mut()),
                GENERIC_WRITE | WRITE_OWNER | WRITE_DAC,
                *mode, // attributes
                FILE_SHARE_DELETE | FILE_SHARE_READ | FILE_SHARE_WRITE,
                *flags as u32, // disposition
                &mut handle as *mut _,
            );

            if res == ERROR_SUCCESS {
                if let FileSecurity::Windows {
                    creator: Some(creator),
                    ..
                } = security
                {
                    use std::ffi::OsString;
                    let sddl = format!("O:{}D:ARAI", creator.to_str().unwrap());
                    debug!(sddl);
                    let security =
                        FileSecurity::from_sddl(OsString::from(sddl));
                    let descriptor = security
                        .to_descriptor()
                        .expect("Failed to make creator descriptor")
                        .unwrap();
                    let info = OWNER_SECURITY_INFORMATION
                        | UNPROTECTED_DACL_SECURITY_INFORMATION
                        | DACL_SECURITY_INFORMATION;
                    res = OpSetFileSecurity(
                        &info as *const _ as *mut _,
                        descriptor.as_ptr() as *mut _,
                        handle,
                    );
                }
            }

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
                    ) as i32
                },
            )
            .map_err(|e| e.raw_os_error().unwrap())
            .err_or_ok()
        }
        VFSCall::truncate(truncate { path, size }) => with_file(
            &translate_path(path, root),
            OpenOptions::new().write(true),
            |handle| OpSetEndOfFile(*size, handle) as i32,
        )
        .map_err(|e| e.raw_os_error().unwrap())
        .err_or_ok(),
        VFSCall::rename(rename { from, to, flags }) => {
            use winapi::um::winnt::DELETE;
            let rto = translate_path(to, root);
            let real_to = path_to_wstr(&rto);
            with_file(
                &translate_path(from, root),
                OpenOptions::new().access_mode(DELETE),
                |handle| {
                    OpMoveFile(real_to.as_ptr(), *flags as i32, handle) as i32
                },
            )
            .map_err(|e| e.raw_os_error().unwrap())
            .err_or_ok()
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
            let descriptor = security
                .clone()
                .to_descriptor()
                .expect("Failed to create security descriptor");
            let mut handle = INVALID_HANDLE_VALUE;
            // Giving it loosest sharing access may not be a good idea, I may
            // need to replicate.
            let mut res = OpCreateDirectory(
                real_path.as_ptr(),
                descriptor
                    .map(|d| d.as_ptr() as *mut _)
                    .unwrap_or(ptr::null_mut()),
                GENERIC_WRITE,
                *mode, // attributes
                FILE_SHARE_DELETE | FILE_SHARE_READ | FILE_SHARE_WRITE,
                CREATE_NEW, // disposition
                &mut handle as *mut _,
            );

            if res == ERROR_SUCCESS {
                if let FileSecurity::Windows {
                    creator: Some(_), ..
                } = security
                {
                    let descriptor = security
                        .creator_descriptor()
                        .expect("Failed to make creator descriptor");
                    let info = OWNER_SECURITY_INFORMATION;
                    res = OpSetFileSecurity(
                        &info as *const _ as *mut _,
                        descriptor.as_ptr() as *mut _,
                        handle,
                    );
                }
            }

            if handle != INVALID_HANDLE_VALUE {
                CloseHandle(handle);
            }
            res as i32
        }
        VFSCall::security(security { path, security }) => {
            use winapi::um::winbase::FILE_FLAG_BACKUP_SEMANTICS;
            use winapi::um::winnt::ACCESS_SYSTEM_SECURITY;
            let info = if let FileSecurity::Windows { info, .. } = security {
                info.unwrap()
            } else {
                panic!("Security information needs translation");
            };

            debug!(security);

            let descriptor = security
                .clone()
                .to_descriptor()
                .expect("Failed to create security descriptor")
                .expect("Descriptor must be present for SetFileSecurity");

            with_file(
                &translate_path(path, root),
                OpenOptions::new()
                    .access_mode(
                        ACCESS_SYSTEM_SECURITY
                            | GENERIC_WRITE
                            | WRITE_DAC
                            | WRITE_OWNER,
                    )
                    .attributes(FILE_FLAG_BACKUP_SEMANTICS),
                |handle| {
                    OpSetFileSecurity(
                        &info as *const _ as *mut _,
                        descriptor.as_ptr() as *mut _,
                        handle,
                    ) as i32
                },
            )
            .map_err(|e| {
                debug!(e);
                e.raw_os_error().unwrap()
            })
            .err_or_ok()
        }
        VFSCall::chmod(chmod { path, mode }) => {
            let rpath = translate_path(path, root);
            let real_path = path_to_wstr(&rpath);
            OpSetFileAttributes(real_path.as_ptr(), *mode as u32) as i32
        }
        VFSCall::fsync(_) => ERROR_SUCCESS as i32, /* Don't need to execute it, just needed for flush synchronous mode */
        _ => panic!("Windows cannot dispatch {:?}, translation required", call),
    }
}
