#![allow(non_camel_case_types)]

use libc::*;

#[repr(C)]
pub struct fuse_file_info {
    pub flags: c_int,
    pub fuse_flags: c_uint,
    pub pad: c_uint, // fuse developers are retards
    pub fh: uint64_t,
    pub lock_owner: uint64_t,
    pub poll_events: uint32_t,
}

pub type fuse_readdir_flags = u32;
pub type fuse_fill_dir_flags = u32;

pub type fuse_fill_dir_t = Option<
    unsafe extern "C" fn(
        buf: *mut c_void,
        name: *const c_char,
        stbuf: *const stat,
        off: off_t,
        flags: fuse_fill_dir_flags,
    ) -> c_int,
>;

pub type fuse_buf_flags = u32;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct fuse_buf {
    pub size: usize,
    pub flags: fuse_buf_flags,
    pub mem: *mut c_void,
    pub fd: c_int,
    pub pos: off_t,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct fuse_bufvec {
    pub count: usize,
    pub idx: usize,
    pub off: usize,
    pub buf: [fuse_buf; 1usize],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct fuse_conn_info {
    pub proto_major: c_uint,
    pub proto_minor: c_uint,
    pub max_write: c_uint,
    pub max_read: c_uint,
    pub max_readahead: c_uint,
    pub capable: c_uint,
    pub want: c_uint,
    pub max_background: c_uint,
    pub congestion_threshold: c_uint,
    pub time_gran: c_uint,
    pub reserved: [c_uint; 22usize],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct fuse_config {
    pub set_gid: c_int,
    pub gid: c_uint,
    pub set_uid: c_int,
    pub uid: c_uint,
    pub set_mode: c_int,
    pub umask: c_uint,
    pub entry_timeout: f64,
    pub negative_timeout: f64,
    pub attr_timeout: f64,
    pub intr: c_int,
    pub intr_signal: c_int,
    pub remember: c_int,
    pub hard_remove: c_int,
    pub use_ino: c_int,
    pub readdir_ino: c_int,
    pub direct_io: c_int,
    pub kernel_cache: c_int,
    pub auto_cache: c_int,
    pub ac_attr_timeout_set: c_int,
    pub ac_attr_timeout: f64,
    pub nullpath_ok: c_int,
    pub show_help: c_int,
    pub modules: *mut c_char,
    pub debug: c_int,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct fuse_operations {
    pub getattr: Option<
        unsafe extern "C" fn(
            arg1: *const c_char,
            arg2: *mut stat,
            fi: *mut fuse_file_info,
        ) -> c_int,
    >,
    pub readlink:
        Option<unsafe extern "C" fn(arg1: *const c_char, arg2: *mut c_char, arg3: usize) -> c_int>,
    pub mknod:
        Option<unsafe extern "C" fn(arg1: *const c_char, arg2: mode_t, arg3: dev_t) -> c_int>,
    pub mkdir: Option<unsafe extern "C" fn(arg1: *const c_char, arg2: mode_t) -> c_int>,
    pub unlink: Option<unsafe extern "C" fn(arg1: *const c_char) -> c_int>,
    pub rmdir: Option<unsafe extern "C" fn(arg1: *const c_char) -> c_int>,
    pub symlink: Option<unsafe extern "C" fn(arg1: *const c_char, arg2: *const c_char) -> c_int>,
    pub rename: Option<
        unsafe extern "C" fn(arg1: *const c_char, arg2: *const c_char, flags: c_uint) -> c_int,
    >,
    pub link: Option<unsafe extern "C" fn(arg1: *const c_char, arg2: *const c_char) -> c_int>,
    pub chmod: Option<
        unsafe extern "C" fn(arg1: *const c_char, arg2: mode_t, fi: *mut fuse_file_info) -> c_int,
    >,
    pub chown: Option<
        unsafe extern "C" fn(
            arg1: *const c_char,
            arg2: uid_t,
            arg3: gid_t,
            fi: *mut fuse_file_info,
        ) -> c_int,
    >,
    pub truncate: Option<
        unsafe extern "C" fn(arg1: *const c_char, arg2: off_t, fi: *mut fuse_file_info) -> c_int,
    >,
    pub open: Option<unsafe extern "C" fn(arg1: *const c_char, arg2: *mut fuse_file_info) -> c_int>,
    pub read: Option<
        unsafe extern "C" fn(
            arg1: *const c_char,
            arg2: *mut c_char,
            arg3: usize,
            arg4: off_t,
            arg5: *mut fuse_file_info,
        ) -> c_int,
    >,
    pub write: Option<
        unsafe extern "C" fn(
            arg1: *const c_char,
            arg2: *const c_uchar,
            arg3: usize,
            arg4: off_t,
            arg5: *mut fuse_file_info,
        ) -> c_int,
    >,
    pub statfs: Option<unsafe extern "C" fn(arg1: *const c_char, arg2: *mut statvfs) -> c_int>,
    pub flush:
        Option<unsafe extern "C" fn(arg1: *const c_char, arg2: *mut fuse_file_info) -> c_int>,
    pub release:
        Option<unsafe extern "C" fn(arg1: *const c_char, arg2: *mut fuse_file_info) -> c_int>,
    pub fsync: Option<
        unsafe extern "C" fn(arg1: *const c_char, arg2: c_int, arg3: *mut fuse_file_info) -> c_int,
    >,
    pub setxattr: Option<
        unsafe extern "C" fn(
            arg1: *const c_char,
            arg2: *const c_char,
            arg3: *const c_uchar,
            arg4: usize,
            arg5: c_int,
        ) -> c_int,
    >,
    pub getxattr: Option<
        unsafe extern "C" fn(
            arg1: *const c_char,
            arg2: *const c_char,
            arg3: *mut c_char,
            arg4: usize,
        ) -> c_int,
    >,
    pub listxattr:
        Option<unsafe extern "C" fn(arg1: *const c_char, arg2: *mut c_char, arg3: usize) -> c_int>,
    pub removexattr:
        Option<unsafe extern "C" fn(arg1: *const c_char, arg2: *const c_char) -> c_int>,
    pub opendir:
        Option<unsafe extern "C" fn(arg1: *const c_char, arg2: *mut fuse_file_info) -> c_int>,
    pub readdir: Option<
        unsafe extern "C" fn(
            arg1: *const c_char,
            arg2: *mut c_void,
            arg3: fuse_fill_dir_t,
            arg4: off_t,
            arg5: *mut fuse_file_info,
            arg6: fuse_readdir_flags,
        ) -> c_int,
    >,
    pub releasedir:
        Option<unsafe extern "C" fn(arg1: *const c_char, arg2: *mut fuse_file_info) -> c_int>,
    pub fsyncdir: Option<
        unsafe extern "C" fn(arg1: *const c_char, arg2: c_int, arg3: *mut fuse_file_info) -> c_int,
    >,
    pub init: Option<
        unsafe extern "C" fn(conn: *mut fuse_conn_info, cfg: *mut fuse_config) -> *mut c_void,
    >,
    pub destroy: Option<unsafe extern "C" fn(private_data: *mut c_void)>,
    pub access: Option<unsafe extern "C" fn(arg1: *const c_char, arg2: c_int) -> c_int>,
    pub create: Option<
        unsafe extern "C" fn(arg1: *const c_char, arg2: mode_t, arg3: *mut fuse_file_info) -> c_int,
    >,
    pub lock: Option<
        unsafe extern "C" fn(
            arg1: *const c_char,
            arg2: *mut fuse_file_info,
            cmd: c_int,
            arg3: *mut flock,
        ) -> c_int,
    >,
    pub utimens: Option<
        unsafe extern "C" fn(
            arg1: *const c_char,
            tv: *const timespec,
            fi: *mut fuse_file_info,
        ) -> c_int,
    >,
    pub bmap:
        Option<unsafe extern "C" fn(arg1: *const c_char, blocksize: usize, idx: *mut u64) -> c_int>,
    pub ioctl: Option<
        unsafe extern "C" fn(
            arg1: *const c_char,
            cmd: c_int,
            arg: *mut c_void,
            arg2: *mut fuse_file_info,
            flags: c_uint,
            data: *mut c_void,
        ) -> c_int,
    >,
    pub poll: Option<
        unsafe extern "C" fn(
            arg1: *const c_char,
            arg2: *mut fuse_file_info,
            ph: *mut c_void, // this should be poll handle
            reventsp: *mut c_uint,
        ) -> c_int,
    >,
    pub write_buf: Option<
        unsafe extern "C" fn(
            arg1: *const c_char,
            buf: *mut fuse_bufvec,
            off: off_t,
            arg2: *mut fuse_file_info,
        ) -> c_int,
    >,
    pub read_buf: Option<
        unsafe extern "C" fn(
            arg1: *const c_char,
            bufp: *mut *mut fuse_bufvec,
            size: usize,
            off: off_t,
            arg2: *mut fuse_file_info,
        ) -> c_int,
    >,
    pub flock: Option<
        unsafe extern "C" fn(arg1: *const c_char, arg2: *mut fuse_file_info, op: c_int) -> c_int,
    >,
    pub fallocate: Option<
        unsafe extern "C" fn(
            arg1: *const c_char,
            arg2: c_int,
            arg3: off_t,
            arg4: off_t,
            arg5: *mut fuse_file_info,
        ) -> c_int,
    >,
}
