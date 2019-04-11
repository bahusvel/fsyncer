use either::Either;
use libc::*;

#[inline]
pub fn neg_errno() -> i32 {
    use errno::errno;
    let errno: i32 = errno().into();
    -errno
}

pub unsafe fn xmp_mknod(
    path: *const c_char,
    mode: mode_t,
    rdev: dev_t,
    uid: uint32_t,
    gid: uint32_t,
) -> c_int {
    let res = if mode & S_IFIFO == S_IFIFO {
        mkfifo(path, mode)
    } else {
        mknod(path, mode, rdev)
    };
    if res == -1 {
        return neg_errno();
    }
    xmp_chown(Either::Left(path), uid, gid)
}
pub unsafe fn xmp_mkdir(
    path: *const c_char,
    mode: mode_t,
    uid: uint32_t,
    gid: uint32_t,
) -> c_int {
    if mkdir(path, mode) == -1 {
        return neg_errno();
    }
    xmp_chown(Either::Left(path), uid, gid)
}
pub unsafe fn xmp_unlink(path: *const c_char) -> c_int {
    if unlink(path) == -1 {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_rmdir(path: *const c_char) -> c_int {
    if rmdir(path) == -1 {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_symlink(
    from: *const c_char,
    to: *const c_char,
    uid: uint32_t,
    gid: uint32_t,
) -> c_int {
    if symlink(from, to) == -1 {
        return neg_errno();
    }
    if lchown(to, uid, gid) == -1 {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_rename(
    from: *const c_char,
    to: *const c_char,
    flags: c_uint,
) -> c_int {
    if flags != 0 {
        return -EINVAL;
    }
    if rename(from, to) == -1 {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_link(
    from: *const c_char,
    to: *const c_char,
    uid: uint32_t,
    gid: uint32_t,
) -> c_int {
    if link(from, to) == -1 {
        return neg_errno();
    }
    xmp_chown(Either::Left(to), uid, gid)
}
pub unsafe fn xmp_chmod(
    path_or_fd: Either<*const c_char, c_int>,
    mode: mode_t,
) -> c_int {
    if match path_or_fd {
        Either::Right(fd) => fchmod(fd, mode),
        Either::Left(path) => {
            assert!(!path.is_null());
            chmod(path, mode)
        }
    } == -1
    {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_chown(
    path_or_fd: Either<*const c_char, c_int>,
    uid: uid_t,
    gid: gid_t,
) -> c_int {
    if match path_or_fd {
        Either::Right(fd) => fchown(fd, uid, gid),
        Either::Left(path) => {
            assert!(!path.is_null());
            chown(path, uid, gid)
        }
    } == -1
    {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_truncate(
    path_or_fd: Either<*const c_char, c_int>,
    size: off_t,
) -> c_int {
    if match path_or_fd {
        Either::Right(fd) => ftruncate(fd, size),
        Either::Left(path) => {
            assert!(!path.is_null());
            truncate64(path, size)
        }
    } == -1
    {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_write(
    buf: *const c_uchar,
    size: usize,
    offset: off_t,
    fd: c_int,
) -> c_int {
    // println!(
    // "Doing write {:?} {:?} {} {} {}",
    // path, buf, size, offset, fd
    // );
    let res = pwrite(fd, buf as *const c_void, size, offset);
    if res == -1 {
        neg_errno()
    } else {
        res as c_int
    }
}
pub unsafe fn xmp_fallocate(
    mode: c_int,
    offset: off_t,
    length: off_t,
    fd: c_int,
) -> c_int {
    if mode != 0 {
        return -EOPNOTSUPP;
    }
    -posix_fallocate(fd, offset, length)
}
pub unsafe fn xmp_setxattr(
    path_or_fd: Either<*const c_char, c_int>,
    name: *const c_char,
    value: *const c_uchar,
    size: usize,
    flags: c_int,
) -> c_int {
    if match path_or_fd {
        Either::Right(fd) => {
            fsetxattr(fd, name, value as *const c_void, size, flags)
        }
        Either::Left(path) => {
            assert!(!path.is_null());
            lsetxattr(path, name, value as *const c_void, size, flags)
        }
    } == -1
    {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_removexattr(
    path_or_fd: Either<*const c_char, c_int>,
    name: *const c_char,
) -> c_int {
    if match path_or_fd {
        Either::Right(fd) => fremovexattr(fd, name),
        Either::Left(path) => {
            assert!(!path.is_null());
            lremovexattr(path, name)
        }
    } == -1
    {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_create(
    path: *const c_char,
    mode: mode_t,
    fd: *mut c_int,
    flags: c_int,
    uid: uint32_t,
    gid: uint32_t,
) -> c_int {
    *fd = open(path, flags, mode);
    if *fd == -1 {
        return neg_errno();
    }
    xmp_chown(Either::Right(*fd), uid, gid)
}
pub unsafe fn xmp_utimens(
    path_or_fd: Either<*const c_char, c_int>,
    ts: *const timespec,
) -> c_int {
    if match path_or_fd {
        Either::Right(fd) => futimens(fd, ts),
        Either::Left(path) => {
            assert!(!path.is_null());
            utimensat(0, path, ts, AT_SYMLINK_NOFOLLOW)
        }
    } == -1
    {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_fsync(isdatasync: c_int, fd: c_int) -> c_int {
    if if isdatasync != 0 {
        fdatasync(fd)
    } else {
        fsync(fd)
    } == -1
    {
        return neg_errno();
    }
    0
}
