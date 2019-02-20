mod encoded;
pub mod ffi;

metablock!(cfg(target_family="unix") {
    mod ops_unix;
    pub use self::ops_unix::*;
});
metablock!(cfg(target_family="windows") {
    mod ops_windows;
    pub use self::ops_windows::*;
});

pub use self::encoded::*;
use self::ffi::*;
use libc::int64_t;
use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io;
use walkdir::WalkDir;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum FsyncerMsg<'a> {
    InitMsg(InitMsg),
    AsyncOp(Cow<'a, VFSCall<'a>>),
    SyncOp(Cow<'a, VFSCall<'a>>, u64),
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

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct InitMsg {
    pub mode: ClientMode,
    pub dsthash: u64,
    pub compress: CompMode,
    pub iolimit_bps: usize,
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

#[cfg(target_family = "unix")]
pub fn hash_metadata(path: &Path) -> Result<u64, io::Error> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let mut hasher = DefaultHasher::new();
    let empty = Path::new("");
    for entry in WalkDir::new(path).sort_by(|a, b| a.file_name().cmp(b.file_name())) {
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

#[cfg(target_os = "windows")]
pub fn hash_metadata(path: &Path) -> Result<u64, io::Error> {
    use std::os::windows::fs::MetadataExt;

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
        stat.file_attributes().hash(&mut hasher);
        if !stat.is_dir() {
            stat.len().hash(&mut hasher);
        }
        stat.modified()?.hash(&mut hasher);
        // TODO check ACLs
    }
    Ok(hasher.finish())
}

pub fn parse_human_size(s: &str) -> Option<usize> {
    Some(match s.chars().last().unwrap() {
        'K' | 'k' => s[..s.len() - 1].parse::<usize>().ok()? * 1024,
        'M' | 'm' => s[..s.len() - 1].parse::<usize>().ok()? * 1024 * 1024,
        'G' | 'g' => s[..s.len() - 1].parse::<usize>().ok()? * 1024 * 1024 * 1024,
        _ => s[..s.len()].parse::<usize>().ok()?,
    })
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
#[allow(non_camel_case_types)]
pub enum VFSCall<'a> {
    mknod(mknod<'a>),
    mkdir(mkdir<'a>),
    unlink(unlink<'a>),
    rmdir(rmdir<'a>),
    symlink(symlink<'a>),
    rename(rename<'a>),
    link(link<'a>),
    chmod(chmod<'a>), // On windows this represents attributes
    chown(chown<'a>),
    truncate(truncate<'a>),
    write(write<'a>),
    fallocate(fallocate<'a>),
    setxattr(setxattr<'a>),
    removexattr(removexattr<'a>),
    create(create<'a>),
    utimens(utimens<'a>),
    fsync(fsync<'a>),
    truncating_write { write: write<'a>, length: int64_t },
    allocation_size(allocation_size<'a>),
    security(security<'a>),
}

use std::ffi::{CStr, CString};
pub fn translate_path(path: &CStr, root: &Path) -> CString {
    let p = path.to_path();
    root.join(if p.starts_with("/") {
        p.strip_prefix("/").unwrap()
    } else {
        p
    })
    .into_cstring()
}
