use common::{neg_errno, translate_path};
use libc::*;
use server::fuseops::{fuse_bufvec, fuse_file_info, fuse_fill_dir_t, fuse_readdir_flags};
use server::SERVER_PATH;
use std::ffi::CStr;
use std::ptr;

extern "C" {
    pub fn xmp_readdir(
        arg1: *const c_char,
        arg2: *mut c_void,
        arg3: fuse_fill_dir_t,
        arg4: off_t,
        arg5: *mut fuse_file_info,
        arg6: fuse_readdir_flags,
    ) -> c_int;
    pub fn xmp_read_buf(
        arg1: *const c_char,
        bufp: *mut *mut fuse_bufvec,
        size: usize,
        off: off_t,
        arg2: *mut fuse_file_info,
    ) -> c_int;
}

#[repr(C)]
struct xmp_dirp {
    dp: *mut DIR,
    entry: *mut dirent,
    offset: off_t,
}

macro_rules! trans_ppath {
    ($path:expr) => {
        translate_path(CStr::from_ptr($path), &SERVER_PATH)
    };
}

pub unsafe extern "C" fn xmp_opendir(path: *const c_char, fi: *mut fuse_file_info) -> c_int {
    let mut d = Box::new(xmp_dirp {
        offset: 0,
        dp: ptr::null_mut(),
        entry: ptr::null_mut(),
    });
    let real_path = trans_ppath!(path);

    d.dp = opendir(real_path.as_ptr());
    if d.dp.is_null() {
        return neg_errno();
    }
    (*fi).fh = Box::into_raw(d) as u64;
    0
}

pub unsafe extern "C" fn xmp_releasedir(_path: *const c_char, fi: *mut fuse_file_info) -> c_int {
    if fi.is_null() {
        panic!("Cannot releasedir by path")
    }
    let d = Box::from_raw((*fi).fh as *mut xmp_dirp);
    closedir(d.dp);
    0
}

pub unsafe extern "C" fn xmp_listxattr(
    path: *const c_char,
    list: *mut c_char,
    size: usize,
) -> c_int {
    let real_path = trans_ppath!(path);
    let res = llistxattr(real_path.as_ptr(), list, size);
    if res == -1 {
        return neg_errno();
    }
    res as i32
}
pub unsafe extern "C" fn xmp_getxattr(
    path: *const c_char,
    name: *const c_char,
    value: *mut c_char,
    size: usize,
) -> c_int {
    let real_path = trans_ppath!(path);
    let res = lgetxattr(real_path.as_ptr(), name, value as *mut _, size);
    if res == -1 {
        return neg_errno();
    }
    res as i32
}

pub unsafe extern "C" fn xmp_release(_path: *const c_char, fi: *mut fuse_file_info) -> c_int {
    if !fi.is_null() {
        close((*fi).fh as i32);
        0
    } else {
        panic!("Cannot release by path");
    }
}

pub unsafe extern "C" fn xmp_flush(_path: *const c_char, fi: *mut fuse_file_info) -> c_int {
    if !fi.is_null() {
        /* This is called from every close on an open file, so call the
        close on the underlying filesystem.	But since flush may be
        called multiple times for an open file, this must not really
        close the file.  This is important if used on a network
        filesystem like NFS which flush the data/metadata on close() */
        if close(dup((*fi).fh as i32)) == -1 {
            return neg_errno();
        }
        0
    } else {
        panic!("Don't know how to flush by path");
    }
}

pub unsafe extern "C" fn xmp_statfs(path: *const c_char, stbuf: *mut statvfs) -> c_int {
    let real_path = trans_ppath!(path);
    if statvfs(real_path.as_ptr(), stbuf) == -1 {
        return neg_errno();
    }
    0
}

pub unsafe extern "C" fn xmp_read(
    path: *const c_char,
    buf: *mut c_char,
    size: usize,
    offset: off_t,
    fi: *mut fuse_file_info,
) -> c_int {
    if !fi.is_null() {
        let res = pread((*fi).fh as i32, buf as *mut _, size, offset);
        if res == -1 {
            return neg_errno();
        }
        res as i32
    } else {
        let real_path = trans_ppath!(path);
        let fd = open(real_path.as_ptr(), O_RDONLY);
        if fd == -1 {
            return neg_errno();
        }
        let res = pread(fd, buf as *mut _, size, offset);
        if res == -1 {
            let err = neg_errno();
            close(fd);
            return err;
        }
        close(fd);
        res as i32
    }
}
pub unsafe extern "C" fn xmp_open(path: *const c_char, fi: *mut fuse_file_info) -> c_int {
    let real_path = trans_ppath!(path);
    let fd = open(real_path.as_ptr(), (*fi).flags);
    if fd == -1 {
        return neg_errno();
    }
    (*fi).fh = fd as u64;
    return 0;
}

pub unsafe extern "C" fn xmp_readlink(path: *const c_char, buf: *mut c_char, size: usize) -> c_int {
    let real_path = trans_ppath!(path);
    let res = readlink(real_path.as_ptr(), buf, size - 1);
    if res == -1 {
        return neg_errno();
    }
    *buf.offset(res) = 0;
    0
}
pub unsafe extern "C" fn xmp_access(path: *const c_char, mask: c_int) -> c_int {
    let real_path = trans_ppath!(path);
    if access(real_path.as_ptr(), mask) == -1 {
        return neg_errno();
    }
    0
}
pub unsafe extern "C" fn xmp_getattr(
    path: *const c_char,
    stbuf: *mut stat,
    fi: *mut fuse_file_info,
) -> c_int {
    let res = if !fi.is_null() {
        fstat((*fi).fh as i32, stbuf)
    } else {
        let real_path = trans_ppath!(path);
        lstat(real_path.as_ptr(), stbuf)
    };
    if res == -1 {
        return neg_errno();
    }
    0
}

/* Cannot be used for arbitrary ioctl
pub unsafe extern "C" fn xmp_ioctl(
    path: *const c_char,
    cmd: c_int,
    arg: *mut c_void,
    arg2: *mut fuse_file_info,
    flags: c_uint,
    data: *mut c_void,
) -> c_int {
    println!("ioctl at {:?}", CStr::from_ptr(path));
    0
}
*/
