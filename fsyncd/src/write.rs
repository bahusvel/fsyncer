use common::*;
use encoded::*;
use libc::*;
use server::{handle_op, server_path_rust};
use std::ffi::{CStr, CString};
use std::ptr;
use std::slice;

#[repr(C)]
pub struct fuse_file_info {
    flags: c_int,
    fuse_flags: c_uint,
    fh: uint64_t,
    lock_owneder: uint64_t,
    poll_events: uint32_t,
}

#[no_mangle]
pub unsafe extern "C" fn do_mknod(path: *const c_char, mode: mode_t, rdev: dev_t) -> c_int {
    let real_path = translate_path(CStr::from_ptr(path), &server_path_rust);
    let res = xmp_mknod(real_path.as_ptr(), mode, rdev);
    handle_op(VFSCall::mknod(mknod {
        path: CString::from(CStr::from_ptr(path)),
        mode,
        rdev,
    }));
    res
}
#[no_mangle]
pub unsafe extern "C" fn do_mkdir(path: *const c_char, mode: mode_t) -> c_int {
    let real_path = translate_path(CStr::from_ptr(path), &server_path_rust);
    let res = xmp_mkdir(real_path.as_ptr(), mode);
    handle_op(VFSCall::mkdir(mkdir {
        path: CString::from(CStr::from_ptr(path)),
        mode,
    }));
    res
}
#[no_mangle]
pub unsafe extern "C" fn do_unlink(path: *const c_char) -> c_int {
    let real_path = translate_path(CStr::from_ptr(path), &server_path_rust);
    let res = xmp_unlink(real_path.as_ptr());
    handle_op(VFSCall::unlink(unlink {
        path: CString::from(CStr::from_ptr(path)),
    }));
    res
}
#[no_mangle]
pub unsafe extern "C" fn do_rmdir(path: *const c_char) -> c_int {
    let real_path = translate_path(CStr::from_ptr(path), &server_path_rust);
    let res = xmp_rmdir(real_path.as_ptr());
    handle_op(VFSCall::rmdir(rmdir {
        path: CString::from(CStr::from_ptr(path)),
    }));
    res
}
#[no_mangle]
pub unsafe extern "C" fn do_symlink(from: *const c_char, to: *const c_char) -> c_int {
    let real_to = translate_path(CStr::from_ptr(to), &server_path_rust);
    let res = xmp_symlink(from, real_to.as_ptr());
    handle_op(VFSCall::symlink(symlink {
        from: CString::from(CStr::from_ptr(from)),
        to: CString::from(CStr::from_ptr(to)),
    }));
    res
}
#[no_mangle]
pub unsafe extern "C" fn do_rename(from: *const c_char, to: *const c_char, flags: c_uint) -> c_int {
    if flags != 0 {
        return -EINVAL;
    }
    let real_from = translate_path(CStr::from_ptr(from), &server_path_rust);
    let real_to = translate_path(CStr::from_ptr(to), &server_path_rust);
    let res = xmp_rename(real_from.as_ptr(), real_to.as_ptr(), flags);
    handle_op(VFSCall::rename(rename {
        from: CString::from(CStr::from_ptr(from)),
        to: CString::from(CStr::from_ptr(to)),
        flags,
    }));
    res
}
#[no_mangle]
pub unsafe extern "C" fn do_link(from: *const c_char, to: *const c_char) -> c_int {
    let real_from = translate_path(CStr::from_ptr(from), &server_path_rust);
    let real_to = translate_path(CStr::from_ptr(to), &server_path_rust);
    let res = xmp_link(real_from.as_ptr(), real_to.as_ptr());
    handle_op(VFSCall::link(link {
        from: CString::from(CStr::from_ptr(from)),
        to: CString::from(CStr::from_ptr(to)),
    }));
    res
}
#[no_mangle]
pub unsafe extern "C" fn do_chmod(
    path: *const c_char,
    mode: mode_t,
    fi: *const fuse_file_info,
) -> c_int {
    let res = if !fi.is_null() {
        let real_path = translate_path(CStr::from_ptr(path), &server_path_rust);
        xmp_chmod(real_path.as_ptr(), mode, -1)
    } else {
        xmp_chmod(ptr::null(), mode, (*fi).fh as c_int)
    };
    handle_op(VFSCall::chmod(chmod {
        path: CString::from(CStr::from_ptr(path)),
        mode,
    }));
    res
}
#[no_mangle]
pub unsafe extern "C" fn do_chown(
    path: *const c_char,
    uid: uid_t,
    gid: gid_t,
    fi: *const fuse_file_info,
) -> c_int {
    let res = if !fi.is_null() {
        let real_path = translate_path(CStr::from_ptr(path), &server_path_rust);
        xmp_chown(real_path.as_ptr(), uid, gid, -1)
    } else {
        xmp_chown(ptr::null(), uid, gid, (*fi).fh as c_int)
    };
    handle_op(VFSCall::chown(chown {
        path: CString::from(CStr::from_ptr(path)),
        uid,
        gid,
    }));
    res
}
#[no_mangle]
pub unsafe extern "C" fn do_truncate(
    path: *const c_char,
    size: off_t,
    fi: *const fuse_file_info,
) -> c_int {
    let res = if !fi.is_null() {
        let real_path = translate_path(CStr::from_ptr(path), &server_path_rust);
        xmp_truncate(real_path.as_ptr(), size, -1)
    } else {
        xmp_truncate(ptr::null(), size, (*fi).fh as c_int)
    };
    handle_op(VFSCall::truncate(truncate {
        path: CString::from(CStr::from_ptr(path)),
        size,
    }));
    res
}
#[no_mangle]
pub unsafe extern "C" fn do_write(
    path: *const c_char,
    buf: *const c_uchar,
    size: usize,
    offset: off_t,
    fi: *const fuse_file_info,
) -> c_int {
    let real_path = translate_path(CStr::from_ptr(path), &server_path_rust);
    let res = if !fi.is_null() {
        let real_path = translate_path(CStr::from_ptr(path), &server_path_rust);
        xmp_write(real_path.as_ptr(), buf, size, offset, -1)
    } else {
        xmp_write(ptr::null(), buf, size, offset, (*fi).fh as c_int)
    };
    handle_op(VFSCall::write(write {
        path: CString::from(CStr::from_ptr(path)),
        buf: Vec::from(slice::from_raw_parts(buf, size)),
        offset,
    }));
    res
}
#[no_mangle]
pub unsafe extern "C" fn do_fallocate(
    path: *const c_char,
    mode: c_int,
    offset: off_t,
    length: off_t,
    fi: *const fuse_file_info,
) -> c_int {
    let real_path = translate_path(CStr::from_ptr(path), &server_path_rust);
    let res = if !fi.is_null() {
        let real_path = translate_path(CStr::from_ptr(path), &server_path_rust);
        xmp_fallocate(real_path.as_ptr(), mode, offset, length, -1)
    } else {
        xmp_fallocate(ptr::null(), mode, offset, length, (*fi).fh as c_int)
    };
    handle_op(VFSCall::fallocate(fallocate {
        path: CString::from(CStr::from_ptr(path)),
        mode,
        offset,
        length,
    }));
    res
}
#[no_mangle]
pub unsafe extern "C" fn do_setxattr(
    path: *const c_char,
    name: *const c_char,
    value: *const c_uchar,
    size: usize,
    flags: c_int,
) -> c_int {
    let real_path = translate_path(CStr::from_ptr(path), &server_path_rust);
    let res = xmp_setxattr(real_path.as_ptr(), name, value, size, flags);
    handle_op(VFSCall::setxattr(setxattr {
        path: CString::from(CStr::from_ptr(path)),
        name: CString::from(CStr::from_ptr(name)),
        value: Vec::from(slice::from_raw_parts(value, size)),
        flags,
    }));
    res
}
#[no_mangle]
pub unsafe extern "C" fn do_removexattr(path: *const c_char, name: *const c_char) -> c_int {
    let real_path = translate_path(CStr::from_ptr(path), &server_path_rust);
    let res = xmp_removexattr(real_path.as_ptr(), name);
    handle_op(VFSCall::removexattr(removexattr {
        path: CString::from(CStr::from_ptr(path)),
        name: CString::from(CStr::from_ptr(name)),
    }));
    res
}
#[no_mangle]
pub unsafe extern "C" fn do_create(
    path: *const c_char,
    mode: mode_t,
    fi: *mut fuse_file_info,
) -> c_int {
    assert!(!fi.is_null());
    let real_path = translate_path(CStr::from_ptr(path), &server_path_rust);
    let res = xmp_create(
        real_path.as_ptr(),
        mode,
        &mut (*fi).fh as *mut uint64_t as *mut c_int,
        (*fi).flags,
    );
    handle_op(VFSCall::create(create {
        path: CString::from(CStr::from_ptr(path)),
        mode,
        flags: (*fi).flags,
    }));
    res
}
#[no_mangle]
pub unsafe extern "C" fn do_utimens(
    path: *const c_char,
    ts: *const timespec,
    fi: *const fuse_file_info,
) -> c_int {
    let real_path = translate_path(CStr::from_ptr(path), &server_path_rust);
    let res = if !fi.is_null() {
        let real_path = translate_path(CStr::from_ptr(path), &server_path_rust);
        xmp_utimens(real_path.as_ptr(), ts, -1)
    } else {
        xmp_utimens(ptr::null(), ts, (*fi).fh as c_int)
    };
    handle_op(VFSCall::utimens(utimens {
        path: CString::from(CStr::from_ptr(path)),
        timespec: [(*ts).into(), (*ts.offset(1)).into()],
    }));
    res
}
