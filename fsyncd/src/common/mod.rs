#![allow(dead_code)]
mod encoded;
pub mod file_security;

metablock!(cfg(target_family="unix") {
    mod ops_unix;
    pub use self::ops_unix::*;
    mod ffi;
    pub use self::ffi::*;
});
metablock!(cfg(target_family="windows") {
    mod ops_windows;
    pub use self::ops_windows::*;
    use std::ffi::{OsString, OsStr};
});

pub use self::encoded::*;
pub use self::file_security::FileSecurity;
use libc::int64_t;
use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::fs::OpenOptions;

use std::hash::{Hash, Hasher};
use std::io::Error;
use std::path::{Path, PathBuf};
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
pub enum ClientAck {
    Ack,
    Dead,
    RetCode(i32),
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct AckMsg {
    pub retcode: ClientAck,
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
pub fn hash_metadata(path: &Path) -> Result<u64, Error> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let mut hasher = DefaultHasher::new();
    let empty = Path::new("");
    for entry in
        WalkDir::new(path).sort_by(|a, b| a.file_name().cmp(b.file_name()))
    {
        let e = entry?;
        let path = e.path().strip_prefix(path).unwrap();
        let stat = e.metadata()?;
        if path == empty && stat.is_dir() {
            continue;
        }
        path.hash(&mut hasher);
        e.file_type().hash(&mut hasher);
        stat.permissions().mode().hash(&mut hasher);
        if !stat.is_dir() {
            stat.len().hash(&mut hasher);
        }
        //stat.modified()?.hash(&mut hasher); This will result in insync
        // clusters reporting different hashes, as the times are not precisely
        // replicated during writes. Once there is an option to precisely
        // replicate timestamps I can re-enable this option.
        stat.uid().hash(&mut hasher);
        stat.gid().hash(&mut hasher);
    }
    Ok(hasher.finish())
}

#[cfg(target_os = "windows")]
pub fn hash_metadata(path: &Path) -> Result<u64, Error> {
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
        'G' | 'g' => {
            s[..s.len() - 1].parse::<usize>().ok()? * 1024 * 1024 * 1024
        }
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
    truncate(truncate<'a>),
    write(write<'a>),
    fallocate(fallocate<'a>),
    setxattr(setxattr<'a>),
    removexattr(removexattr<'a>),
    create(create<'a>),
    utimens(utimens<'a>),
    fsync(fsync<'a>),
    truncating_write { write: write<'a>, length: int64_t },
    security(security<'a>), //chown on linux
}

pub fn translate_path(path: &Path, root: &Path) -> PathBuf {
    root.join(if path.starts_with("/") {
        path.strip_prefix("/").unwrap()
    } else {
        path
    })
}
metablock!(cfg(target_family = "unix") {
    use std::ffi::{CStr, CString};
    pub fn trans_cstr(path: &CStr, root: &Path) -> CString {
        translate_path(path.to_path(), root).into_cstring()
    }
    pub fn canonize_path(path: &Path) -> Result<PathBuf, Error> {
        path.canonicalize()
    }
    pub fn with_file<T, F: (FnOnce(i32) -> T)>(
        path: &Path,
        options: &OpenOptions,
        f: F,
    ) -> Result<T, Error> {
        use std::fs::File;
        use std::os::unix::io::{FromRawFd, IntoRawFd};
        let file = options.open(path)?;
        let handle = file.into_raw_fd();
        let res = f(handle);
        unsafe { File::from_raw_fd(handle) };
        Ok(res)
    }
});

metablock!(cfg(target_os = "windows") {
    pub unsafe fn wstr_to_os(s: LPCWSTR) -> OsString {
        use std::os::windows::ffi::OsStringExt;
        let len = libc::wcslen(s);
        OsString::from_wide(std::slice::from_raw_parts(s, len))
    }
    pub fn os_to_wstr(s: &OsStr) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        let mut buf: Vec<u16> = s.encode_wide().collect();
        buf.push(0);
        buf
    }
    pub unsafe fn wstr_to_path(path: LPCWSTR) -> PathBuf {
        PathBuf::from(wstr_to_os(path))
    }
    pub fn path_to_wstr(path: &Path) -> Vec<u16> {
        os_to_wstr(path.as_os_str())
    }
    pub unsafe fn trans_wstr(path: LPCWSTR, root: &Path) -> Vec<u16> {
        path_to_wstr(&translate_path(&wstr_to_path(path), root))
    }
    pub fn canonize_path(path: &Path) -> Result<PathBuf, Error> {
        // Rust implementation of Path::canonicalize() on windows is retarded, hence this
        use winapi::um::fileapi::GetFullPathNameW;
        use std::ptr;
        const MAX_PATH: usize = 32767; // Supports the longest path
        let wpath = path_to_wstr(path);
        let mut buf: [u16; MAX_PATH] = [0; MAX_PATH];
        buf[0] = '\\' as u16;
        buf[1] = '\\' as u16;
        buf[2] = '?' as u16;
        buf[3] = '\\' as u16;
        if unsafe {GetFullPathNameW(wpath.as_ptr(), buf.len() as u32,  buf.as_mut_ptr().offset(4), ptr::null_mut())} == 0 {
            return Err(Error::last_os_error());
        }
        Ok(unsafe {wstr_to_path(buf[..].as_mut_ptr())})
    }
    pub fn with_file<T, F: (FnOnce(HANDLE) -> T)>(
        path: &Path,
        options: &OpenOptions,
        f: F,
    ) -> Result<T, Error> {
        use std::fs::File;
        use std::os::windows::io::{FromRawHandle, IntoRawHandle};
        let file = options.open(path)?;
        let handle = file.into_raw_handle();
        let res = f(handle);
        unsafe { File::from_raw_handle(handle) };
        Ok(res)
    }
    use std::ffi::c_void;
    use std::ops::{Deref, DerefMut, Drop};
    pub struct WinapiBox<T> {
        t: *mut T,
        borrows: Vec<WinapiBox<c_void>>,
    }
    impl<T> Deref for WinapiBox<T> {
        type Target = T;
        fn deref(&self) -> &T {
            unsafe { &(*self.t) }
        }
    }
    impl<T> DerefMut for WinapiBox<T> {
        fn deref_mut(&mut self) -> &mut T {
            unsafe { &mut (*self.t) }
        }
    }
    impl<T> Drop for WinapiBox<T> {
        fn drop(&mut self) {
            use winapi::um::winbase::LocalFree;
            unsafe { LocalFree(self.t as *mut _) };
        }
    }
    impl<T> WinapiBox<T> {
        pub fn add_borrow<O>(&mut self, b: WinapiBox<O>) {
            self.borrows
                .push(unsafe { WinapiBox::from_raw(b.as_ptr() as *mut c_void) });
            std::mem::forget(b); // Forget the box which has been added as a borrow
        }
        pub unsafe fn from_raw(t: *mut T) -> Self {
            assert!(!t.is_null());
            WinapiBox {
                t,
                borrows: Vec::new(),
            }
        }
        pub fn as_ptr(&self) -> *mut T {
            self.t
        }
    }
});

pub struct Lazy<T, F: FnOnce() -> T> {
    f: Option<F>,
    t: Option<T>,
}

impl<T, F: FnOnce() -> T> Lazy<T, F> {
    pub fn new(f: F) -> Self {
        Lazy {
            f: Some(f),
            t: None,
        }
    }
    pub fn get_or_create(&mut self) -> &mut T {
        match self.t {
            Some(ref mut t) => t,
            None => {
                self.t = Some((self.f.take().unwrap())());
                self.t.as_mut().unwrap()
            }
        }
    }
    pub fn take(mut self) -> T {
        match self.t {
            Some(t) => t,
            None => (self.f.take().unwrap())(),
        }
    }
}

pub trait ErrorOrOk<T> {
    fn err_or_ok(self) -> T;
}

impl<T> ErrorOrOk<T> for Result<T, T> {
    fn err_or_ok(self) -> T {
        match self {
            Err(t) | Ok(t) => t,
        }
    }
}
