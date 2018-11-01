use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use walkdir::WalkDir;

#[repr(C)]
#[derive(PartialEq, Clone, Copy)]
#[allow(dead_code)]
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
    NOP,
}

#[derive(PartialEq, Clone, Copy)]
#[allow(non_camel_case_types)]
pub enum ClientMode {
    MODE_ASYNC,
    MODE_SYNC,
    MODE_SEMISYNC,
    MODE_CONTROL,
    MODE_LISTENER,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct op_msg {
    pub op_length: u32,
    pub op_type: op_type,
    pub tid: u64,
}

pub struct InitMsg {
    pub mode: ClientMode,
    pub dsthash: u64,
    pub compress: CompMode,
}

pub struct AckMsg {
    pub retcode: i32,
    pub tid: u64,
}

bitflags! {
    pub struct CompMode: u32 {
        const RT_DSSC_ZLIB      = 0b00001;
        const RT_DSSC_CHUNKED   = 0b00010;
        const RT_DSSC_ZSTD      = 0b00100;
        const STREAM_ZSTD       = 0b01000;
        const STREAM_LZ4        = 0b10000;
    }
}

pub fn hash_metadata(path: &str) -> Result<u64, io::Error> {
    let mut hasher = DefaultHasher::new();
    let empty = Path::new("");
    for entry in WalkDir::new(path) {
        let e = entry?;
        let path = e.path().strip_prefix(path).unwrap();
        if path == empty {
            continue;
        }
        path.hash(&mut hasher);
        e.file_type().hash(&mut hasher);
        let stat = e.metadata()?;
        stat.permissions().mode().hash(&mut hasher);
        if !stat.is_dir() {
            stat.len().hash(&mut hasher);
        }
        stat.modified()?.hash(&mut hasher);
        stat.uid().hash(&mut hasher);
        stat.gid().hash(&mut hasher);
    }
    Ok(hasher.finish())
}

use libc::*;
extern "C" {
    pub fn xmp_mknod(path: *const c_char, mode: mode_t, rdev: dev_t) -> c_int;
    pub fn xmp_mkdir(path: *const c_char, mode: mode_t) -> c_int;
    pub fn xmp_unlink(path: *const c_char) -> c_int;
    pub fn xmp_rmdir(path: *const c_char) -> c_int;
    pub fn xmp_symlink(from: *const c_char, to: *const c_char) -> c_int;
    pub fn xmp_rename(from: *const c_char, to: *const c_char, flags: c_uint) -> c_int;
    pub fn xmp_link(from: *const c_char, to: *const c_char) -> c_int;
    pub fn xmp_chmod(path: *const c_char, mode: mode_t, fd: c_int) -> c_int;
    pub fn xmp_chown(path: *const c_char, uid: uid_t, gid: gid_t, fd: c_int) -> c_int;
    pub fn xmp_truncate(path: *const c_char, size: off_t, fd: c_int) -> c_int;
    pub fn xmp_write(
        path: *const c_char,
        buf: *const c_uchar,
        size: usize,
        offset: off_t,
        fd: c_int,
    ) -> c_int;
    pub fn xmp_fallocate(
        path: *const c_char,
        mode: c_int,
        offset: off_t,
        length: off_t,
        fd: c_int,
    ) -> c_int;
    pub fn xmp_setxattr(
        path: *const c_char,
        name: *const c_char,
        value: *const c_uchar,
        size: usize,
        flags: c_int,
    ) -> c_int;
    pub fn xmp_removexattr(path: *const c_char, name: *const c_char) -> c_int;
    pub fn xmp_create(path: *const c_char, mode: mode_t, fd: *mut c_int, flags: c_int) -> c_int;
    pub fn xmp_utimens(path: *const c_char, ts: *const timespec, fd: c_int) -> c_int;
}

use encoded::*;
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum VFSCall {
    mknod(mknod),
    mkdir(mkdir),
    unlink(unlink),
    rmdir(rmdir),
    symlink(symlink),
    rename(rename),
    link(link),
    chmod(chmod),
    chown(chown),
    truncate(truncate),
    write(write),
    fallocate(fallocate),
    setxattr(setxattr),
    removexattr(removexattr),
    create(create),
    utimens(utimens),
}

use std::ffi::{CStr, CString};
pub fn translate_path(path: &CStr, root: &str) -> CString {
    let mut vec = root.as_bytes().to_vec();
    vec.extend_from_slice(path.to_bytes());
    // It is impossible for the new string to contain a zero byte, hence bellow result may be unwrapped
    CString::new(vec).unwrap()
}
