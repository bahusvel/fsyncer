use common::*;
use either::Either;
use libc::c_int;
use std::fs::OpenOptions;
use std::path::Path;

pub unsafe fn dispatch(call: &VFSCall, root: &Path) -> c_int {
    use libc::*;
    match call {
        VFSCall::mknod(mknod {
            path,
            mode,
            rdev,
            security: FileSecurity::Unix { uid, gid },
        }) => {
            let path = translate_path(&path, root);
            xmp_mknod(path.into_cstring().as_ptr(), *mode, *rdev, *uid, *gid)
        }
        VFSCall::mkdir(mkdir {
            path,
            mode,
            security: FileSecurity::Unix { uid, gid },
        }) => {
            let path = translate_path(&path, root);
            xmp_mkdir(path.into_cstring().as_ptr(), *mode, *uid, *gid)
        }
        VFSCall::unlink(unlink { path }) => {
            let path = translate_path(&path, root);
            xmp_unlink(path.into_cstring().as_ptr())
        }
        VFSCall::rmdir(rmdir { path }) => {
            let path = translate_path(&path, root);
            xmp_rmdir(path.into_cstring().as_ptr())
        }
        VFSCall::symlink(symlink {
            from,
            to,
            security: FileSecurity::Unix { uid, gid },
        }) => {
            let to = translate_path(&to, root);
            xmp_symlink(
                from.clone().into_owned().into_cstring().as_ptr(),
                to.into_cstring().as_ptr(),
                *uid,
                *gid,
            )
        }
        VFSCall::rename(rename { from, to, flags }) => {
            let from = translate_path(&from, root);
            let to = translate_path(&to, root);
            xmp_rename(
                from.into_cstring().as_ptr(),
                to.into_cstring().as_ptr(),
                *flags,
            )
        }
        VFSCall::link(link {
            from,
            to,
            security: FileSecurity::Unix { uid, gid },
        }) => {
            let from = translate_path(&from, root);
            let to = translate_path(&to, root);
            xmp_link(
                from.into_cstring().as_ptr(),
                to.into_cstring().as_ptr(),
                *uid,
                *gid,
            )
        }
        VFSCall::chmod(chmod { path, mode }) => {
            let path = translate_path(&path, root);
            xmp_chmod(Either::Left(path.into_cstring().as_ptr()), *mode)
        }
        VFSCall::security(security {
            path,
            security: FileSecurity::Unix { uid, gid },
        }) => {
            let path = translate_path(&path, root);
            xmp_chown(Either::Left(path.into_cstring().as_ptr()), *uid, *gid)
        }
        VFSCall::truncate(truncate { path, size }) => {
            let path = translate_path(&path, root);
            xmp_truncate(Either::Left(path.into_cstring().as_ptr()), *size)
        }
        VFSCall::write(write { path, buf, offset }) => with_file(
            &translate_path(&path, root),
            OpenOptions::new().write(true),
            |fd| xmp_write(buf.as_ptr(), buf.len(), *offset, fd),
        )
        .map_err(|e| e.raw_os_error().unwrap())
        .err_or_ok(),
        VFSCall::truncating_write {
            write: write { path, buf, offset },
            length,
        } => with_file(
            &translate_path(&path, root),
            OpenOptions::new().write(true),
            |fd| {
                let res = xmp_write(buf.as_ptr(), buf.len(), *offset, fd);
                if res < 0 {
                    return res;
                }
                let tres = xmp_truncate(Either::Right(fd), *length);
                if tres < 0 {
                    return tres;
                }
                res
            },
        )
        .map_err(|e| e.raw_os_error().unwrap())
        .err_or_ok(),
        VFSCall::fallocate(fallocate {
            path,
            mode,
            offset,
            length,
        }) => with_file(
            &translate_path(&path, root),
            OpenOptions::new().write(true),
            |fd| xmp_fallocate(*mode, *offset, *length, fd),
        )
        .map_err(|e| e.raw_os_error().unwrap())
        .err_or_ok(),
        VFSCall::setxattr(setxattr {
            path,
            name,
            value,
            flags,
        }) => {
            let path = translate_path(&path, root);
            xmp_setxattr(
                Either::Left(path.into_cstring().as_ptr()),
                name.as_ptr(),
                value.as_ptr(),
                value.len(),
                *flags,
            )
        }
        VFSCall::removexattr(removexattr { path, name }) => {
            let path = translate_path(&path, root);
            xmp_removexattr(
                Either::Left(path.into_cstring().as_ptr()),
                name.as_ptr(),
            )
        }
        VFSCall::create(create {
            path,
            mode,
            flags,
            security: FileSecurity::Unix { uid, gid },
        }) => {
            let path = translate_path(&path, root);
            let mut fd = -1;
            let res = xmp_create(
                path.into_cstring().as_ptr(),
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
            let ts = [timespec[0].into(), timespec[1].into()];
            xmp_utimens(
                Either::Left(path.into_cstring().as_ptr()),
                &ts as *const timespec,
            )
        }
        VFSCall::fsync(fsync { path, isdatasync }) => with_file(
            &translate_path(&path, root),
            OpenOptions::new().write(true),
            |fd| xmp_fsync(*isdatasync, fd),
        )
        .map_err(|e| e.raw_os_error().unwrap())
        .err_or_ok(),
        _ => panic!("Not implemented"),
    }
}
