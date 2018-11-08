use libc::*;

#[inline]
pub fn neg_errno() -> i32 {
    use errno::errno;
    let errno: i32 = errno().into();
    -errno
}

pub unsafe fn xmp_mknod(path: *const c_char, mode: mode_t, rdev: dev_t) -> c_int {
    let res = if mode & S_IFIFO == S_IFIFO {
        mkfifo(path, mode)
    } else {
        mknod(path, mode, rdev)
    };
    if res == -1 {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_mkdir(path: *const c_char, mode: mode_t) -> c_int {
    if mkdir(path, mode) == -1 {
        return neg_errno();
    }
    0
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
pub unsafe fn xmp_symlink(from: *const c_char, to: *const c_char) -> c_int {
    if symlink(from, to) == -1 {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_rename(from: *const c_char, to: *const c_char, flags: c_uint) -> c_int {
    if flags != 0 {
        return -EINVAL;
    }
    if rename(from, to) == -1 {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_link(from: *const c_char, to: *const c_char) -> c_int {
    if link(from, to) == -1 {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_chmod(path: *const c_char, mode: mode_t, fd: c_int) -> c_int {
    let res = if path.is_null() {
        fchmod(fd, mode)
    } else {
        chmod(path, mode)
    };
    if res == -1 {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_chown(path: *const c_char, uid: uid_t, gid: gid_t, fd: c_int) -> c_int {
    let res = if path.is_null() {
        fchown(fd, uid, gid)
    } else {
        chown(path, uid, gid)
    };
    if res == -1 {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_truncate(path: *const c_char, size: off_t, fd: c_int) -> c_int {
    let res = if path.is_null() {
        ftruncate(fd, size)
    } else {
        truncate64(path, size)
    };
    if res == -1 {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_write(
    path: *const c_char,
    buf: *const c_uchar,
    size: usize,
    offset: off_t,
    mut fd: c_int,
) -> c_int {
    // println!(
    // "Doing write {:?} {:?} {} {} {}",
    // path, buf, size, offset, fd
    // );
    let mut opened = false;
    if !path.is_null() {
        fd = open(path, O_WRONLY);
        if fd == -1 {
            return neg_errno();
        }
        opened = true;
    }

    let res = pwrite(fd, buf as *const c_void, size, offset);
    if res == -1 {
        let errno = neg_errno();
        if opened {
            close(fd);
        }
        return errno;
    }

    if opened {
        close(fd);
    }
    return res as c_int;
}
pub unsafe fn xmp_fallocate(
    path: *const c_char,
    mode: c_int,
    offset: off_t,
    length: off_t,
    mut fd: c_int,
) -> c_int {
    let mut opened = false;

    if mode != 0 {
        return -EOPNOTSUPP;
    }

    if !path.is_null() {
        fd = open(path, O_WRONLY);
        if fd == -1 {
            return neg_errno();
        }
        opened = true;
    }

    let res = -posix_fallocate(fd, offset, length);
    if opened {
        close(fd);
    }
    return res as c_int;
}
pub unsafe fn xmp_setxattr(
    path: *const c_char,
    name: *const c_char,
    value: *const c_uchar,
    size: usize,
    flags: c_int,
) -> c_int {
    if lsetxattr(path, name, value as *const c_void, size, flags) == -1 {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_removexattr(path: *const c_char, name: *const c_char) -> c_int {
    if lremovexattr(path, name) == -1 {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_create(path: *const c_char, mode: mode_t, fd: *mut c_int, flags: c_int) -> c_int {
    *fd = open(path, flags, mode);
    if *fd == -1 {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_utimens(path: *const c_char, ts: *const timespec, fd: c_int) -> c_int {
    let res = if path.is_null() {
        futimens(fd, ts)
    } else {
        utimensat(0, path, ts, AT_SYMLINK_NOFOLLOW)
    };
    if res == -1 {
        return neg_errno();
    }
    0
}
pub unsafe fn xmp_fsync(path: *const c_char, isdatasync: c_int, mut fd: c_int) -> c_int {
    let mut opened = false;

    if !path.is_null() {
        fd = open(path, O_WRONLY);
        if fd == -1 {
            return neg_errno();
        }
        opened = true;
    }

    let res = if isdatasync != 0 {
        fdatasync(fd)
    } else {
        fsync(fd)
    };

    if opened {
        close(fd);
    }

    if res == -1 {
        return neg_errno();
    }
    0
}
