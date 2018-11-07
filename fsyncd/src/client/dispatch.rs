use common::*;
use libc::*;

pub unsafe fn dispatch(call: VFSCall, root: &str) -> c_int {
    match call {
        VFSCall::mknod(mknod { path, mode, rdev }) => {
            let path = translate_path(&path, root);
            xmp_mknod(path.as_ptr(), mode, rdev)
        }
        VFSCall::mkdir(mkdir { path, mode }) => {
            let path = translate_path(&path, root);
            xmp_mkdir(path.as_ptr(), mode)
        }
        VFSCall::unlink(unlink { path }) => {
            let path = translate_path(&path, root);
            xmp_unlink(path.as_ptr())
        }
        VFSCall::rmdir(rmdir { path }) => {
            let path = translate_path(&path, root);
            xmp_rmdir(path.as_ptr())
        }
        VFSCall::symlink(symlink { from, to }) => {
            let to = translate_path(&to, root);
            xmp_symlink(from.as_ptr(), to.as_ptr())
        }
        VFSCall::rename(rename { from, to, flags }) => {
            let from = translate_path(&from, root);
            let to = translate_path(&to, root);
            xmp_rename(from.as_ptr(), to.as_ptr(), flags)
        }
        VFSCall::link(link { from, to }) => {
            let from = translate_path(&from, root);
            let to = translate_path(&to, root);
            xmp_link(from.as_ptr(), to.as_ptr())
        }
        VFSCall::chmod(chmod { path, mode }) => {
            let path = translate_path(&path, root);
            xmp_chmod(path.as_ptr(), mode, -1)
        }
        VFSCall::chown(chown { path, uid, gid }) => {
            let path = translate_path(&path, root);
            xmp_chown(path.as_ptr(), uid, gid, -1)
        }
        VFSCall::truncate(truncate { path, size }) => {
            let path = translate_path(&path, root);
            xmp_truncate(path.as_ptr(), size, -1)
        }
        VFSCall::write(write { path, buf, offset }) => {
            let path = translate_path(&path, root);
            xmp_write(path.as_ptr(), buf.as_ptr(), buf.len(), offset, -1)
        }
        VFSCall::fallocate(fallocate {
            path,
            mode,
            offset,
            length,
        }) => {
            let path = translate_path(&path, root);
            xmp_fallocate(path.as_ptr(), mode, offset, length, -1)
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
                flags,
            )
        }
        VFSCall::removexattr(removexattr { path, name }) => {
            let path = translate_path(&path, root);
            xmp_removexattr(path.as_ptr(), name.as_ptr())
        }
        VFSCall::create(create { path, mode, flags }) => {
            let path = translate_path(&path, root);
            let mut fd = -1;
            let res = xmp_create(path.as_ptr(), mode, &mut fd as *mut c_int, flags);
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
    }
}