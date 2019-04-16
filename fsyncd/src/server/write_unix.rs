use common::*;
use either::Either;
use libc::*;
use server::fusemain::fuse_get_context;
use server::fuseops::fuse_file_info;
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
    let call = VFSCall::mknod(mknod {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        mode,
        rdev,
        security: FileSecurity::Unix {
            uid: (*context).uid,
            gid: (*context).gid,
        },
    });

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
    let call = VFSCall::mkdir(mkdir {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        mode,
        security: FileSecurity::Unix {
            uid: (*context).uid,
            gid: (*context).gid,
        },
    });

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
    let call = VFSCall::unlink(unlink {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
    });

    let opref = pre_op(&call);
    if let Some(r) = opref.ret {
        return r;
    }
    let res = xmp_unlink(real_path.as_ptr());
    post_op(opref, res)
}

pub unsafe extern "C" fn do_rmdir(path: *const c_char) -> c_int {
    let real_path = trans_ppath!(path);
    let call = VFSCall::rmdir(rmdir {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
    });

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
    let call = VFSCall::symlink(symlink {
        from: Cow::Borrowed(CStr::from_ptr(from).to_path()),
        to: Cow::Borrowed(CStr::from_ptr(to).to_path()),
        security: FileSecurity::Unix {
            uid: (*context).uid,
            gid: (*context).gid,
        },
    });

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
    let call = VFSCall::rename(rename {
        from: Cow::Borrowed(CStr::from_ptr(from).to_path()),
        to: Cow::Borrowed(CStr::from_ptr(to).to_path()),
        flags,
    });

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
    let call = VFSCall::link(link {
        from: Cow::Borrowed(CStr::from_ptr(from).to_path()),
        to: Cow::Borrowed(CStr::from_ptr(to).to_path()),
        security: FileSecurity::Unix {
            uid: (*context).uid,
            gid: (*context).gid,
        },
    });

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
    let call = VFSCall::chmod(chmod {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        mode,
    });

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
    let call = VFSCall::security(security {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        security: FileSecurity::Unix { uid: uid, gid: gid },
    });

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
    let call = VFSCall::truncate(truncate {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        size,
    });

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
    let call = VFSCall::write(write {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        buf: Cow::Borrowed(slice::from_raw_parts(buf, size)),
        offset,
    });
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
    let call = VFSCall::fallocate(fallocate {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        mode,
        offset,
        length,
    });
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
    let call = VFSCall::setxattr(setxattr {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        name: Cow::Borrowed(CStr::from_ptr(name)),
        value: Cow::Borrowed(slice::from_raw_parts(value, size)),
        flags,
    });

    //println!("setxattr {:?}", call);

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
    let call = VFSCall::removexattr(removexattr {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        name: Cow::Borrowed(CStr::from_ptr(name)),
    });

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

    let call = VFSCall::create(create {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        mode,
        flags: (*fi).flags,
        security: FileSecurity::Unix {
            uid: (*context).uid,
            gid: (*context).gid,
        },
    });

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
    //println!("Created {:?} {}", real_path, fd);
    post_op(opref, res)
}

pub unsafe extern "C" fn do_utimens(
    path: *const c_char,
    ts: *const timespec,
    fi: *mut fuse_file_info,
) -> c_int {
    let call = VFSCall::utimens(utimens {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        timespec: [
            (*ts).into(),
            (*ts.offset(1)).into(),
            enc_timespec { high: 0, low: 0 },
        ],
    });

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
    //println!("Sync received");
    let call = VFSCall::fsync(fsync {
        path: Cow::Borrowed(CStr::from_ptr(path).to_path()),
        isdatasync: isdatasync,
    });
    let opref = pre_op(&call);
    if let Some(r) = opref.ret {
        return r;
    }
    assert!(!fi.is_null());
    post_op(opref, xmp_fsync(isdatasync, (*fi).fh as c_int))
}
