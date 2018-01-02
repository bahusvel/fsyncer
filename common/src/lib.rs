#![feature(libc)]
extern crate libc;

use libc::{c_char};
use std::ffi::CString;

#[repr(C)]
#[derive(PartialEq, Clone, Copy)]
pub enum op_type {
    MKNOD,
	MKDIR,
	UNLINK,
	RMDIR,
	SYMLINK,
	RENAME,
	LINK,
	CHMOD,
	CHOWN,
	TRUNCATE,
	WRITE,
	FALLOCATE,
	SETXATTR,
	REMOVEXATTR,
	CREATE,
	UTIMENS,
}

#[repr(C)]
#[derive(PartialEq, Clone, Copy)]
#[allow(non_camel_case_types)]
pub enum client_mode { MODE_ASYNC, MODE_SYNC, MODE_CONTROL }

#[repr(C)]
pub struct op_msg {
    pub op_length: u32,
    pub op_type: op_type,
}

#[repr(C)]
pub struct init_msg {
    pub mode: client_mode,
    pub dsthash: u64,
}

#[repr(C)]
pub struct ack_msg {
	pub retcode: i32
}

#[link(name = "fsyncer_common", kind = "static")]
extern {
    fn hash_metadata(path: *const c_char) -> u64;
}

pub fn hash_mdata(path: &str) -> u64 {
    let s = CString::new(path).unwrap();
    unsafe {hash_metadata(s.as_ptr())}
}
