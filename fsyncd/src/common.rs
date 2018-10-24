use walkdir::WalkDir;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hasher, Hash};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

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
}

#[derive(PartialEq, Clone, Copy)]
#[allow(non_camel_case_types)]
pub enum client_mode {
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

pub struct init_msg {
    pub mode: client_mode,
    pub dsthash: u64,
    pub compress: CompMode,
}

pub struct ack_msg {
    pub retcode: i32,
    pub tid: u64,
}

bitflags! {
    pub struct CompMode: u32 {
        const RT_DSSC_ZLIB = 0b0001;
        const RT_DSSC_CHUNKED = 0b0010;
        const RT_DSSC_ZSTD = 0b0100;
        const STREAM_ZSTD = 0b1000;
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
