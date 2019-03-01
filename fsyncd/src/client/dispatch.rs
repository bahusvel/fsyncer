use common::*;
use libc::c_int;
use std::path::Path;

#[cfg(target_family = "unix")]
pub unsafe fn dispatch(call: &VFSCall, root: &Path) -> c_int {
    use libc::*;
    match call {
        VFSCall::mknod(mknod {
            path,
            mode,
            rdev,
            uid,
            gid,
        }) => {
            let path = translate_path(&path, root);
            xmp_mknod(path.as_ptr(), *mode, *rdev, *uid, *gid)
        }
        VFSCall::mkdir(mkdir {
            path,
            mode,
            uid,
            gid,
        }) => {
            let path = translate_path(&path, root);
            xmp_mkdir(path.as_ptr(), *mode, *uid, *gid)
        }
        VFSCall::unlink(unlink { path }) => {
            let path = translate_path(&path, root);
            xmp_unlink(path.as_ptr())
        }
        VFSCall::rmdir(rmdir { path }) => {
            let path = translate_path(&path, root);
            xmp_rmdir(path.as_ptr())
        }
        VFSCall::symlink(symlink { from, to, uid, gid }) => {
            let to = translate_path(&to, root);
            xmp_symlink(from.as_ptr(), to.as_ptr(), *uid, *gid)
        }
        VFSCall::rename(rename { from, to, flags }) => {
            let from = translate_path(&from, root);
            let to = translate_path(&to, root);
            xmp_rename(from.as_ptr(), to.as_ptr(), *flags)
        }
        VFSCall::link(link { from, to, uid, gid }) => {
            let from = translate_path(&from, root);
            let to = translate_path(&to, root);
            xmp_link(from.as_ptr(), to.as_ptr(), *uid, *gid)
        }
        VFSCall::chmod(chmod { path, mode }) => {
            let path = translate_path(&path, root);
            xmp_chmod(path.as_ptr(), *mode, -1)
        }
        VFSCall::chown(chown { path, uid, gid }) => {
            let path = translate_path(&path, root);
            xmp_chown(path.as_ptr(), *uid, *gid, -1)
        }
        VFSCall::truncate(truncate { path, size }) => {
            let path = translate_path(&path, root);
            xmp_truncate(path.as_ptr(), *size, -1)
        }
        VFSCall::write(write { path, buf, offset }) => {
            let path = translate_path(&path, root);
            xmp_write(path.as_ptr(), buf.as_ptr(), buf.len(), *offset, -1)
        }
        VFSCall::truncating_write {
            write: write { path, buf, offset },
            length,
        } => {
            let path = translate_path(&path, root);
            let res = xmp_write(path.as_ptr(), buf.as_ptr(), buf.len(), *offset, -1);
            if res < 0 {
                return res;
            }
            let tres = xmp_truncate(path.as_ptr(), *length, -1);
            if tres < 0 {
                return tres;
            }
            res
        }
        VFSCall::fallocate(fallocate {
            path,
            mode,
            offset,
            length,
        }) => {
            let path = translate_path(&path, root);
            xmp_fallocate(path.as_ptr(), *mode, *offset, *length, -1)
        }
        VFSCall::setxattr(setxattr {
            path,
            name,
            value,
            flags,
        }) => {
            let path = translate_path(&path, root);
            xmp_setxattr(
                path.as_ptr(),
                name.as_ptr(),
                value.as_ptr(),
                value.len(),
                *flags,
            )
        }
        VFSCall::removexattr(removexattr { path, name }) => {
            let path = translate_path(&path, root);
            xmp_removexattr(path.as_ptr(), name.as_ptr())
        }
        VFSCall::create(create {
            path,
            mode,
            flags,
            uid,
            gid,
        }) => {
            let path = translate_path(&path, root);
            let mut fd = -1;
            let res = xmp_create(
                path.as_ptr(),
                *mode,
                &mut fd as *mut c_int,
                *flags,
                *uid,
                *gid,
            );
            if fd != -1 {
                close(fd);
            }
            res
        }
        VFSCall::utimens(utimens { path, timespec }) => {
            let path = translate_path(&path, root);
            let ts = [
                timespec {
                    tv_sec: timespec[0].tv_sec,
                    tv_nsec: timespec[0].tv_nsec,
                },
                timespec {
                    tv_sec: timespec[1].tv_sec,
                    tv_nsec: timespec[1].tv_nsec,
                },
            ];
            xmp_utimens(path.as_ptr(), &ts as *const timespec, -1)
        }
        VFSCall::fsync(fsync { path, isdatasync }) => {
            let path = translate_path(&path, root);
            xmp_fsync(path.as_ptr(), *isdatasync, -1)
        }
    }
}

#[cfg(target_os = "windows")]
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
            let rpath = translate_path(path, root);
            let real_path = path_to_wstr(&rpath);
            let created = timespec[0].clone().into();
            let accessed = timespec[1].clone().into();
            let written = timespec[2].clone().into();
            OpSetFileTime(
                real_path.as_ptr(),
                &created as *const FILETIME,
                &accessed as *const FILETIME,
                &written as *const FILETIME,
                INVALID_HANDLE_VALUE,
            )
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
            res
        }
        VFSCall::write(write { path, buf, offset }) => {
            let rpath = translate_path(path, root);
            let real_path = path_to_wstr(&rpath);
            let mut bytes_written: u32 = 0;
            OpWriteFile(
                real_path.as_ptr(),
                buf.as_ptr() as *const _,
                buf.len() as u32,
                &mut bytes_written as *mut _,
                *offset,
                INVALID_HANDLE_VALUE,
            )
        }
        VFSCall::truncate(truncate { path, size }) => {
            let rpath = translate_path(path, root);
            let real_path = path_to_wstr(&rpath);
            OpSetEndOfFile(real_path.as_ptr(), *size, INVALID_HANDLE_VALUE)
        }
        VFSCall::chmod(chmod { path, mode }) => {
            let rpath = translate_path(path, root);
            let real_path = path_to_wstr(&rpath);
            OpSetFileAttributes(real_path.as_ptr(), *mode)
        }
        VFSCall::rename(rename { from, to, flags }) => {
            let rfrom = translate_path(from, root);
            let real_from = path_to_wstr(&rfrom);
            let rto = translate_path(to, root);
            let real_to = path_to_wstr(&rto);
            OpMoveFile(
                real_from.as_ptr(),
                real_to.as_ptr(),
                *flags as i32,
                INVALID_HANDLE_VALUE,
            )
        }
        VFSCall::rmdir(rmdir { path }) => {
            let rpath = translate_path(path, root);
            let real_path = path_to_wstr(&rpath);
            OpDeleteDirectory(real_path.as_ptr())
        }
        VFSCall::unlink(unlink { path }) => {
            let rpath = translate_path(path, root);
            let real_path = path_to_wstr(&rpath);
            OpDeleteFile(real_path.as_ptr())
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
            res
        }
        VFSCall::security(security { path, security }) => {
            let rpath = translate_path(path, root);
            let real_path = path_to_wstr(&rpath);
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
            OpSetFileSecurity(
                real_path.as_ptr(),
                &mut info as *mut _,
                ptr::null_mut(), // FIXME
                INVALID_HANDLE_VALUE,
            )
        }
        VFSCall::fsync(fsync { path, .. }) => {
            let rpath = translate_path(path, root);
            let real_path = path_to_wstr(&rpath);
            OpFlushFileBuffers(real_path.as_ptr(), INVALID_HANDLE_VALUE)
        }
        VFSCall::allocation_size(allocation_size { path, size }) => {
            let rpath = translate_path(path, root);
            let real_path = path_to_wstr(&rpath);
            OpSetAllocationSize(real_path.as_ptr(), *size, INVALID_HANDLE_VALUE)
        }
        _ => panic!("Windows cannot dispatch {:?}, translation required", call),
    }
}
