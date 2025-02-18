use super::fuse_get_context;
use super::fuseops::fuse_file_info;
use common::*;
use either::Either;
use libc::*;
use server::{post_op, pre_op, SERVER_PATH};
use std::borrow::Cow;
use std::ffi::CStr;
use std::slice;

pub unsafe extern "C" fn do_mknod(
    path: *const c_char,
    mode: mode_t,
    rdev: dev_t,
) -> c_int {
    let real_path = trans_ppath!(path);
    let context = fuse_get_context();
    let call = VFSCall::mknod {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        mode,
        rdev,
        security: FileSecurity::Unix {
            uid: (*context).uid,
            gid: (*context).gid,
        },
    };

    let opref = pre_op(&call);
    if let Some(r) = opref.ret {
        return r;
    }
    let res = xmp_mknod(
        real_path.as_ptr(),
        mode,
        rdev,
        (*context).uid,
        (*context).gid,
    );
    post_op(opref, res)
}

pub unsafe extern "C" fn do_mkdir(path: *const c_char, mode: mode_t) -> c_int {
    let real_path = trans_ppath!(path);
    let context = fuse_get_context();
    let call = VFSCall::mkdir {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        mode,
        security: FileSecurity::Unix {
            uid: (*context).uid,
            gid: (*context).gid,
        },
    };

    let opref = pre_op(&call);
    if let Some(r) = opref.ret {
        return r;
    }
    let res =
        xmp_mkdir(real_path.as_ptr(), mode, (*context).uid, (*context).gid);
    post_op(opref, res)
}

pub unsafe extern "C" fn do_unlink(path: *const c_char) -> c_int {
    let real_path = trans_ppath!(path);
    let call = VFSCall::unlink {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
    };

    let opref = pre_op(&call);
    if let Some(r) = opref.ret {
        return r;
    }
    let res = xmp_unlink(real_path.as_ptr());
    post_op(opref, res)
}

pub unsafe extern "C" fn do_rmdir(path: *const c_char) -> c_int {
    let real_path = trans_ppath!(path);
    let call = VFSCall::rmdir {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
    };

    let opref = pre_op(&call);
    if let Some(r) = opref.ret {
        return r;
    }
    let res = xmp_rmdir(real_path.as_ptr());
    post_op(opref, res)
}

pub unsafe extern "C" fn do_symlink(
    from: *const c_char,
    to: *const c_char,
) -> c_int {
    let real_to = trans_ppath!(to);
    let context = fuse_get_context();
    let call = VFSCall::symlink {
        from: Cow::Borrowed(CStr::from_ptr(from).to_path()),
        to: Cow::Borrowed(CStr::from_ptr(to).to_path()),
        security: FileSecurity::Unix {
            uid: (*context).uid,
            gid: (*context).gid,
        },
    };

    let opref = pre_op(&call);
    if let Some(r) = opref.ret {
        return r;
    }
    let res =
        xmp_symlink(from, real_to.as_ptr(), (*context).uid, (*context).gid);
    post_op(opref, res)
}

pub unsafe extern "C" fn do_rename(
    from: *const c_char,
    to: *const c_char,
    flags: c_uint,
) -> c_int {
    let real_from = trans_ppath!(from);
    let real_to = trans_ppath!(to);
    let call = VFSCall::rename {
        from: Cow::Borrowed(CStr::from_ptr(from).to_path()),
        to: Cow::Borrowed(CStr::from_ptr(to).to_path()),
        flags,
    };

    let opref = pre_op(&call);
    if let Some(r) = opref.ret {
        return r;
    }
    let res = xmp_rename(real_from.as_ptr(), real_to.as_ptr(), flags);
    post_op(opref, res)
}

pub unsafe extern "C" fn do_link(
    from: *const c_char,
    to: *const c_char,
) -> c_int {
    let real_from = trans_ppath!(from);
    let real_to = trans_ppath!(to);
    let context = fuse_get_context();
    let call = VFSCall::link {
        from: Cow::Borrowed(CStr::from_ptr(from).to_path()),
        to: Cow::Borrowed(CStr::from_ptr(to).to_path()),
        security: FileSecurity::Unix {
            uid: (*context).uid,
            gid: (*context).gid,
        },
    };

    let opref = pre_op(&call);
    if let Some(r) = opref.ret {
        return r;
    }
    let res = xmp_link(
        real_from.as_ptr(),
        real_to.as_ptr(),
        (*context).uid,
        (*context).gid,
    );
    post_op(opref, res)
}

pub unsafe extern "C" fn do_chmod(
    path: *const c_char,
    mode: mode_t,
    fi: *mut fuse_file_info,
) -> c_int {
    let call = VFSCall::chmod {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        mode,
    };

    let opref = pre_op(&call);
    if let Some(r) = opref.ret {
        return r;
    }
    let res = if fi.is_null() {
        let real_path = trans_ppath!(path);
        xmp_chmod(Either::Left(real_path.as_ptr()), mode)
    } else {
        xmp_chmod(Either::Right((*fi).fh as c_int), mode)
    };
    post_op(opref, res)
}

pub unsafe extern "C" fn do_chown(
    path: *const c_char,
    uid: uid_t,
    gid: gid_t,
    fi: *mut fuse_file_info,
) -> c_int {
    let call = VFSCall::security {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        security: FileSecurity::Unix { uid, gid },
    };

    let opref = pre_op(&call);
    if let Some(r) = opref.ret {
        return r;
    }
    let res = if fi.is_null() {
        let real_path = trans_ppath!(path);
        xmp_chown(Either::Left(real_path.as_ptr()), uid, gid)
    } else {
        xmp_chown(Either::Right((*fi).fh as c_int), uid, gid)
    };
    post_op(opref, res)
}

pub unsafe extern "C" fn do_truncate(
    path: *const c_char,
    size: off_t,
    fi: *mut fuse_file_info,
) -> c_int {
    let call = VFSCall::truncate {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        size,
    };

    let opref = pre_op(&call);
    if let Some(r) = opref.ret {
        return r;
    }
    let res = if fi.is_null() {
        let real_path = trans_ppath!(path);
        xmp_truncate(Either::Left(real_path.as_ptr()), size)
    } else {
        xmp_truncate(Either::Right((*fi).fh as c_int), size)
    };
    post_op(opref, res)
}

pub unsafe extern "C" fn do_write(
    path: *const c_char,
    buf: *const c_uchar,
    size: usize,
    offset: off_t,
    fi: *mut fuse_file_info,
) -> c_int {
    use server::DIFF_WRITES;
    let cpath = CStr::from_ptr(path).to_path();
    let new_buf = slice::from_raw_parts_mut(buf as *mut u8, size);
    let call = if DIFF_WRITES {
        let file = std::fs::File::from_raw_fd((*fi).fh as c_int);
        use std::os::unix::fs::FileExt;
        use std::os::unix::io::FromRawFd;
        let mut old_buf = Vec::with_capacity(size);
        old_buf.set_len(size);
        let res = file.read_at(&mut old_buf[..], offset as u64);
        std::mem::forget(file);
        match res {
            Ok(0) | Err(_) => {
                // Optimisation, there is no overlap, or:
                // Cannot perform diff write, file may not be readable
                VFSCall::write {
                    path: Cow::Borrowed(cpath),
                    buf: Cow::Borrowed(new_buf),
                    offset,
                }
            }
            Ok(diff_len) => {
                xor_buf(&mut new_buf[..diff_len], &old_buf[..diff_len]);
                let leading_zeroes =
                    new_buf.iter().take_while(|i| **i == 0).count();
                let trailing_zeroes = if diff_len == new_buf.len() {
                    new_buf.iter().rev().take_while(|i| **i == 0).count()
                } else {
                    0 // Zeroes past the end of the file extend the file
                };
                VFSCall::diff_write {
                    path: Cow::Borrowed(cpath),
                    buf: Cow::Borrowed(
                        &new_buf
                            [leading_zeroes..new_buf.len() - trailing_zeroes],
                    ),
                    offset: offset + leading_zeroes as i64,
                }
            }
        }
    } else {
        VFSCall::write {
            path: Cow::Borrowed(cpath),
            buf: Cow::Borrowed(new_buf),
            offset,
        }
    };
    let opref = pre_op(&call);
    if let Some(r) = opref.ret {
        return r;
    }
    assert!(!fi.is_null());
    post_op(opref, xmp_write(buf, size, offset, (*fi).fh as c_int))
}

pub unsafe extern "C" fn do_fallocate(
    path: *const c_char,
    mode: c_int,
    offset: off_t,
    length: off_t,
    fi: *mut fuse_file_info,
) -> c_int {
    let call = VFSCall::fallocate {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        mode,
        offset,
        length,
    };
    let opref = pre_op(&call);
    if let Some(r) = opref.ret {
        return r;
    }
    assert!(!fi.is_null());
    post_op(
        opref,
        xmp_fallocate(mode, offset, length, (*fi).fh as c_int),
    )
}

pub unsafe extern "C" fn do_setxattr(
    path: *const c_char,
    name: *const c_char,
    value: *const c_uchar,
    size: usize,
    flags: c_int,
) -> c_int {
    let real_path = trans_ppath!(path);
    let call = VFSCall::setxattr {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        name: Cow::Borrowed(CStr::from_ptr(name)),
        value: Cow::Borrowed(slice::from_raw_parts(value, size)),
        flags,
    };

    //eprintln!("setxattr {:?}", call);

    let opref = pre_op(&call);
    if let Some(r) = opref.ret {
        return r;
    }
    post_op(
        opref,
        xmp_setxattr(
            Either::Left(real_path.as_ptr()),
            name,
            value,
            size,
            flags,
        ),
    )
}

pub unsafe extern "C" fn do_removexattr(
    path: *const c_char,
    name: *const c_char,
) -> c_int {
    let real_path = trans_ppath!(path);
    let call = VFSCall::removexattr {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        name: Cow::Borrowed(CStr::from_ptr(name)),
    };

    let opref = pre_op(&call);
    if let Some(r) = opref.ret {
        return r;
    }
    post_op(
        opref,
        xmp_removexattr(Either::Left(real_path.as_ptr()), name),
    )
}

pub unsafe extern "C" fn do_create(
    path: *const c_char,
    mode: mode_t,
    fi: *mut fuse_file_info,
) -> c_int {
    assert!(!fi.is_null());
    let real_path = trans_ppath!(path);
    let context = fuse_get_context();

    let call = VFSCall::create {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        mode,
        flags: (*fi).flags,
        security: FileSecurity::Unix {
            uid: (*context).uid,
            gid: (*context).gid,
        },
    };

    let opref = pre_op(&call);
    if let Some(r) = opref.ret {
        return r;
    }
    let mut fd = 0;
    let res = xmp_create(
        real_path.as_ptr(),
        mode,
        &mut fd,
        (*fi).flags,
        (*context).uid,
        (*context).gid,
    );
    (*fi).fh = fd as u64;
    //eprintln!("Created {:?} {}", real_path, fd);
    post_op(opref, res)
}

pub unsafe extern "C" fn do_utimens(
    path: *const c_char,
    ts: *const timespec,
    fi: *mut fuse_file_info,
) -> c_int {
    let call = VFSCall::utimens {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        timespec: [
            (*ts).into(),
            (*ts.offset(1)).into(),
            Timespec { high: 0, low: 0 },
        ],
    };

    let opref = pre_op(&call);
    if let Some(r) = opref.ret {
        return r;
    }
    let res = if fi.is_null() {
        let real_path = trans_ppath!(path);
        xmp_utimens(Either::Left(real_path.as_ptr()), ts)
    } else {
        xmp_utimens(Either::Right((*fi).fh as c_int), ts)
    };
    post_op(opref, res)
}

pub unsafe extern "C" fn do_fsync(
    path: *const c_char,
    isdatasync: c_int,
    fi: *mut fuse_file_info,
) -> c_int {
    debug!("fsync", (*fi).fuse_flags);
    let call = VFSCall::fsync {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        isdatasync,
    };
    let opref = pre_op(&call);
    if let Some(r) = opref.ret {
        return r;
    }
    assert!(!fi.is_null());
    post_op(opref, xmp_fsync(isdatasync, (*fi).fh as c_int))
}
