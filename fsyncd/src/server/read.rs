use common::{neg_errno, translate_path};
use libc::*;
use server::fuseops::{
    fuse_bufvec, fuse_config, fuse_conn_info, fuse_file_info, fuse_fill_dir_t, fuse_readdir_flags,
};
use server::SERVER_PATH_RUST;
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

pub unsafe extern "C" fn xmp_init(conn: *mut fuse_conn_info, cfg: *mut fuse_config) -> *mut c_void {
    (*cfg).use_ino = 1;
    // NOTE this makes path NULL to parameters where fi->fh exists. This is evil
    // for the current case of replication. But in future when this is properly
    // handled it can improve performance.
    // refer to
    // https://libfuse.github.io/doxygen/structfuse__config.html#adc93fd1ac03d7f016d6b0bfab77f3863
    // cfg->nullpath_ok = 1;

    /* Pick up changes from lower filesystem right away. This is
	   also necessary for better hardlink support. When the kernel
	   calls the unlink() handler, it does not know the inode of
	   the to-be-removed entry and can therefore not invalidate
	   the cache of the associated inode - resulting in an
	   incorrect st_nlink value being reported for any remaining
	   hardlinks to this inode. */
    // cfg->entry_timeout = 0;
    // cfg->attr_timeout = 0;
    // cfg->negative_timeout = 0;
    (*cfg).auto_cache = 1;
    (*conn).max_write = 32 * 1024;

    return ptr::null_mut();
}

#[repr(C)]
struct xmp_dirp {
    dp: *mut DIR,
    entry: *mut dirent,
    offset: off_t,
}

pub unsafe extern "C" fn xmp_opendir(path: *const c_char, fi: *mut fuse_file_info) -> c_int {
    let mut d = Box::new(xmp_dirp {
        offset: 0,
        dp: ptr::null_mut(),
        entry: ptr::null_mut(),
    });
    let real_path = translate_path(CStr::from_ptr(path), &SERVER_PATH_RUST);

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
    let real_path = translate_path(CStr::from_ptr(path), &SERVER_PATH_RUST);
    if llistxattr(real_path.as_ptr(), list, size) == -1 {
        return neg_errno();
    }
    0
}
pub unsafe extern "C" fn xmp_getxattr(
    path: *const c_char,
    name: *const c_char,
    value: *mut c_char,
    size: usize,
) -> c_int {
    let real_path = translate_path(CStr::from_ptr(path), &SERVER_PATH_RUST);
    if lgetxattr(real_path.as_ptr(), name, value as *mut _, size) == -1 {
        return neg_errno();
    }
    0
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
    let real_path = translate_path(CStr::from_ptr(path), &SERVER_PATH_RUST);
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
        let real_path = translate_path(CStr::from_ptr(path), &SERVER_PATH_RUST);
        let fd = open(real_path.as_ptr(), O_RDONLY);
        if fd == -1 {
            return neg_errno();
        }
        let res = pread(fd, buf as *mut _, size, offset);
        if res == -1 {
            close(fd);
            return neg_errno();
        }
        close(fd);
        res as i32
    }
}
pub unsafe extern "C" fn xmp_open(path: *const c_char, fi: *mut fuse_file_info) -> c_int {
    let real_path = translate_path(CStr::from_ptr(path), &SERVER_PATH_RUST);
    let fd = open(real_path.as_ptr(), (*fi).flags);
    if fd == -1 {
        return neg_errno();
    }
    (*fi).fh = fd as u64;
    return 0;
}

pub unsafe extern "C" fn xmp_readlink(path: *const c_char, buf: *mut c_char, size: usize) -> c_int {
    let real_path = translate_path(CStr::from_ptr(path), &SERVER_PATH_RUST);
    let res = readlink(real_path.as_ptr(), buf, size - 1);
    if res == -1 {
        return neg_errno();
    }
    *buf.offset(res) = 0;
    0
}
pub unsafe extern "C" fn xmp_access(path: *const c_char, mask: c_int) -> c_int {
    let real_path = translate_path(CStr::from_ptr(path), &SERVER_PATH_RUST);
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
        let real_path = translate_path(CStr::from_ptr(path), &SERVER_PATH_RUST);
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
