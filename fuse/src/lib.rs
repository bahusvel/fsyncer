#![allow(
    non_upper_case_globals,
    unused_variables,
    non_camel_case_types,
    non_snake_case
)]
#![feature(trace_macros)]

extern crate libc;
#[macro_use]
mod ops;

use libc::EOPNOTSUPP;
use libc::{stat, timespec};
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::path::Path;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[macro_export]
macro_rules! FUSE_BUFVEC_INIT {
    ($size:expr) => {
        fuse_bufvec {
            count: 1,
            idx: 0,
            off: 0,
            buf: [fuse_buf {
                size: $size,
                flags: 0,
                mem: ptr::null_mut(),
                fd: -1,
                pos: 0,
            }],
        }
    };
}

pub enum FuseReply {
    attr(stat, f64),
    bmap(u64),
    buf(Vec<u8>, usize),
    create(fuse_entry_param, *mut fuse_file_info),
    data(fuse_bufvec, fuse_buf_copy_flags),
    entry(fuse_entry_param),
    err(c_int),
    ioctl(c_int, Vec<u8>, usize),
    ioctl_iov(c_int, iovec, c_int),
    ioctl_retry(iovec, usize, iovec, usize),
    iov(iovec, c_int),
    lock(flock),
    none(),
    open(*mut fuse_file_info),
    poll(c_uint),
    readlink(CString),
    statfs(statvfs),
    write(usize),
    xattr(usize),
}

impl FuseReply {
    fn send(self, req: fuse_req_t) -> i32 {
        unsafe {
            match self {
                FuseReply::attr(attr, attr_timeout) => {
                    fuse_reply_attr(req, &attr, attr_timeout)
                }
                FuseReply::bmap(idx) => fuse_reply_bmap(req, idx),
                FuseReply::buf(buf, size) => {
                    fuse_reply_buf(req, buf.as_ptr() as *const i8, size)
                }
                FuseReply::create(e, fi) => fuse_reply_create(req, &e, fi),
                FuseReply::data(mut bufv, flags) => {
                    fuse_reply_data(req, &mut bufv, flags)
                }
                FuseReply::entry(e) => fuse_reply_entry(req, &e),
                FuseReply::err(err) => fuse_reply_err(req, err),
                FuseReply::ioctl(result, buf, size) => {
                    fuse_reply_ioctl(req, result, buf.as_ptr() as _, size)
                }
                FuseReply::ioctl_iov(result, mut iov, count) => {
                    fuse_reply_ioctl_iov(req, result, &mut iov, count)
                }
                FuseReply::ioctl_retry(
                    in_iov,
                    in_count,
                    out_iov,
                    out_count,
                ) => fuse_reply_ioctl_retry(
                    req, &in_iov, in_count, &out_iov, out_count,
                ),
                FuseReply::iov(iov, count) => fuse_reply_iov(req, &iov, count),
                FuseReply::lock(lock) => fuse_reply_lock(req, &lock),
                FuseReply::none() => {
                    fuse_reply_none(req);
                    0
                }
                FuseReply::open(fi) => fuse_reply_open(req, fi),
                FuseReply::poll(revents) => fuse_reply_poll(req, revents),
                FuseReply::readlink(link) => {
                    fuse_reply_readlink(req, link.as_ptr())
                }
                FuseReply::statfs(stbuf) => fuse_reply_statfs(req, &stbuf),
                FuseReply::write(count) => fuse_reply_write(req, count),
                FuseReply::xattr(count) => fuse_reply_xattr(req, count),
            }
        }
    }
}

// macro_rules! allowed_replies {
//     ($reply:expr, $t1:path, $($t:path),*) => {
//         match &$reply => {
//             $t1 $(| $t(_))* => {},
//             _ => panic!("{} received invalid reply (not of type {})",
// stringify!($reply), concat!($t1 $(, stringify!($t))*))         }
//     };
// }

macro_rules! declare_method {
    ($method:ident, $($arg_name:ident: $arg_type:ty),+) => {
        unsafe fn $method(&self, $($arg_name: $arg_type),+) -> FuseReply {
            FuseReply::err(EOPNOTSUPP)
        }
    };
}
pub trait FilesystemLL {
    unsafe fn init(
        &mut self,
        userdata: *mut c_void,
        conn: *mut fuse_conn_info,
    ) {
    }
    unsafe fn destroy(&self, userdata: *mut c_void) {}
    unsafe fn forget(&self, ino: fuse_ino_t, nlookup: u64) {}
    unsafe fn forget_multi(
        &self,
        count: usize,
        forgets: *mut fuse_forget_data,
    ) {
        for i in 0..count {
            let forget = forgets.add(i);
            self.forget((*forget).ino, (*forget).nlookup);
        }
    }
    unsafe fn setattr(
        &self,
        ino: fuse_ino_t,
        attr: *mut stat,
        to_set: c_int,
        fi: *mut fuse_file_info,
    ) -> FuseReply {
        if to_set as u32 & FUSE_SET_ATTR_MODE != 0 {
            if let FuseReply::err(e) = self.chmod(ino, (*attr).st_mode, fi) {
                return FuseReply::err(e);
            }
        }
        if to_set as u32 & (FUSE_SET_ATTR_UID | FUSE_SET_ATTR_GID) != 0 {
            let uid = if to_set as u32 & FUSE_SET_ATTR_UID != 0 {
                (*attr).st_uid
            } else {
                -1i32 as u32
            };
            let gid = if to_set as u32 & FUSE_SET_ATTR_GID != 0 {
                (*attr).st_gid
            } else {
                -1i32 as u32
            };
            if let FuseReply::err(e) = self.chown(ino, uid, gid, fi) {
                return FuseReply::err(e);
            }
        }
        if to_set as u32 & FUSE_SET_ATTR_SIZE != 0 {
            if let FuseReply::err(e) = self.truncate(ino, (*attr).st_size, fi) {
                return FuseReply::err(e);
            }
        }
        if to_set as u32 & (FUSE_SET_ATTR_ATIME | FUSE_SET_ATTR_MTIME) != 0 {
            let ts = [
                timespec {
                    tv_sec: 0,
                    tv_nsec: if to_set as u32 & FUSE_SET_ATTR_ATIME_NOW != 0 {
                        libc::UTIME_NOW
                    } else if to_set as u32 & FUSE_SET_ATTR_ATIME != 0 {
                        (*attr).st_atime
                    } else {
                        libc::UTIME_OMIT
                    },
                },
                timespec {
                    tv_sec: 0,
                    tv_nsec: if to_set as u32 & FUSE_SET_ATTR_MTIME_NOW != 0 {
                        libc::UTIME_NOW
                    } else if to_set as u32 & FUSE_SET_ATTR_MTIME != 0 {
                        (*attr).st_mtime
                    } else {
                        libc::UTIME_OMIT
                    },
                },
            ];
            if let FuseReply::err(e) = self.utimens(ino, ts.as_ptr(), fi) {
                return FuseReply::err(e);
            }
        }
        return self.getattr(ino, fi);
    }
    op_list!(generic, op_args, declare_method);
    op_list!(withreq, op_args, declare_method);
    op_list!(attr, op_args, declare_method);
}

macro_rules! proxy_generic {
    ($method:ident, req: fuse_req_t, $($arg_name:ident: $arg_type:ty),+) => {
        unsafe extern "C" fn $method(req: fuse_req_t, $($arg_name: $arg_type),+){
            FS.as_ref().unwrap().$method(req, $($arg_name),+).send(req);
        }
    };
    ($method:ident, $($arg_name:ident: $arg_type:ty),+) => {
        unsafe extern "C" fn $method(req: fuse_req_t, $($arg_name: $arg_type),+){
            FS.as_ref().unwrap().$method($($arg_name),+).send(req);
        }
    };

}
macro_rules! proxy_nonereply {
    ($method:ident, $($arg_name:ident: $arg_type:ty),+) => {
        unsafe extern "C" fn $method(req: fuse_req_t, $($arg_name: $arg_type),+){
            FS.as_ref().unwrap().$method($($arg_name),+);
            FuseReply::none().send(req);
        }
    };
}
macro_rules! proxy_fsmut {
    ($method:ident, $($arg_name:ident: $arg_type:ty),+) => {
        unsafe extern "C" fn $method($($arg_name: $arg_type),+){
            FS.as_mut().unwrap().$method($($arg_name),+);
        }
    };
}

op_list!(generic, op_args, proxy_generic);
op_list!(withreq, op_args, proxy_generic);
op_list!(nonereply, op_args, proxy_nonereply);
op_list!(fsmut, op_args, proxy_fsmut);
op_args!(setattr, proxy_generic);

struct NopFs;
impl FilesystemLL for NopFs {}
static mut FS: Option<Box<dyn FilesystemLL>> = None;

pub fn display_fuse_help() {
    unsafe {
        fuse_cmdline_help();
        fuse_lowlevel_help();
    }
}

struct FuseSession {
    ptr: *mut fuse_session,
    signals: bool,
    mounted: bool,
}

impl std::ops::Drop for FuseSession {
    fn drop(&mut self) {
        unsafe {
            if self.mounted {
                fuse_session_unmount(self.ptr);
            }
            if self.signals {
                fuse_remove_signal_handlers(self.ptr);
            }
            fuse_session_destroy(self.ptr);
        }
    }
}

pub fn mount<F: 'static + FilesystemLL, A: IntoIterator<Item = String>>(
    path: &Path,
    mut fs: F,
    extra_args: A,
    threads: u32,
) -> i32 {
    use std::mem;
    let cpath = CString::new(path.to_str().unwrap()).unwrap();
    unsafe { FS = Some(Box::new(NopFs)) };
    let mut ops: fuse_lowlevel_ops = unsafe { mem::zeroed() };
    macro_rules! assign_op {
        ($op:ident) => {
            ops.$op = Some($op);
        };
    }
    op_list!(all, assign_op);
    ops.setattr = Some(setattr);

    let mut args = vec![
        "fsyncd".to_string(),
        String::from(path.to_str().expect("Mount path is not a valid string")),
        "-o".to_string(),
        "default_permissions".to_string(),
    ]
    .into_iter()
    .chain(extra_args.into_iter())
    .map(|arg| CString::new(arg).unwrap())
    .collect::<Vec<CString>>();
    // convert the strings to raw pointers
    let mut c_args = args
        .iter_mut()
        .map(|arg| arg.as_ptr() as *mut c_char)
        .collect::<Vec<*mut c_char>>();

    let mut fuse_args = fuse_args {
        argc: c_args.len() as i32,
        argv: c_args.as_mut_ptr(),
        allocated: 0,
    };
    let mut opts: fuse_cmdline_opts = unsafe { mem::zeroed() };
    if unsafe { fuse_parse_cmdline(&mut fuse_args, &mut opts) } != 0 {
        panic!("Failed to parse fuse cmdline");
    }

    let mut se = FuseSession {
        ptr: unsafe {
            fuse_session_new(
                &mut fuse_args,
                &ops,
                mem::size_of::<fuse_lowlevel_ops>(),
                &mut fs as *mut dyn FilesystemLL as *mut _,
            )
        },
        mounted: false,
        signals: false,
    };

    if se.ptr.is_null() {
        panic!("Could not create fuse session");
    }

    if unsafe { fuse_set_signal_handlers(se.ptr) } != 0 {
        panic!("Failed to parse fuse cmdline");
    }
    se.signals = true;

    unsafe { libc::umask(0) };

    let mut loop_config = fuse_loop_config {
        clone_fd: 0,
        max_idle_threads: threads,
    };

    if unsafe { fuse_session_mount(se.ptr, cpath.as_ptr()) } != 0 {
        panic!("Failed to mount");
    }
    se.mounted = true;

    return if threads == 1 {
        unsafe { fuse_session_loop(se.ptr) }
    } else {
        unsafe { fuse_session_loop_mt(se.ptr, &mut loop_config) }
    };
}
