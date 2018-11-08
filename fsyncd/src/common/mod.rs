mod encoded;
mod ops;

pub use self::encoded::*;
pub use self::ops::*;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum FsyncerMsg {
    InitMsg(InitMsg),
    AsyncOp(VFSCall),
    SyncOp(VFSCall, u64),
    Ack(AckMsg),
    Cork(u64),
    AckCork(u64),
    Uncork,
    NOP,
}

#[derive(PartialEq, Clone, Copy, Serialize, Deserialize, Debug)]
#[allow(non_camel_case_types)]
pub enum ClientMode {
    MODE_ASYNC,
    MODE_SYNC,
    MODE_SEMISYNC,
    MODE_FLUSHSYNC,
    MODE_CONTROL,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct InitMsg {
    pub mode: ClientMode,
    pub dsthash: u64,
    pub compress: CompMode,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct AckMsg {
    pub retcode: i32,
    pub tid: u64,
}

bitflags! {
    #[derive(Serialize, Deserialize)]
    pub struct CompMode: u32 {
        const RT_DSSC_ZLIB      = 0b00001;
        const RT_DSSC_CHUNKED   = 0b00010;
        const RT_DSSC_ZSTD      = 0b00100;
        const STREAM_ZSTD       = 0b01000;
        const STREAM_LZ4        = 0b10000;
        const STREAM_MASK       = 0b11000;
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

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
#[allow(non_camel_case_types)]
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
    fsync(fsync),
}

use std::ffi::{CStr, CString};
pub fn translate_path(path: &CStr, root: &str) -> CString {
    let mut vec = root.as_bytes().to_vec();
    vec.extend_from_slice(path.to_bytes());
    // It is impossible for the new string to contain a zero byte, hence bellow result may be unwrapped
    CString::new(vec).unwrap()
}
