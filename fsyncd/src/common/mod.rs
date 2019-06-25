#![allow(dead_code)]
pub mod file_security;

metablock!(cfg(target_family="unix") {
    mod ops_unix;
    pub use self::ops_unix::*;
    mod ffi;
    pub use self::ffi::*;
    pub mod rsync;
});
metablock!(cfg(target_family="windows") {
    mod ops_windows;
    pub use self::ops_windows::*;
    use common::FILETIME;
    use std::ffi::{OsString, OsStr};
});

pub use self::file_security::FileSecurity;
use libc::*;
use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::ffi::{CStr, CString};
use std::fs::OpenOptions;
use std::hash::{Hash, Hasher};
use std::io::Error;
use std::ops::BitXor;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy, Hash)]
pub struct Timespec {
    pub high: i64,
    pub low: i64,
}

impl BitXor for Timespec {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self {
        Timespec {
            high: self.high ^ rhs.high,
            low: self.low ^ rhs.low,
        }
    }
}

#[cfg(target_family = "unix")]
impl From<timespec> for Timespec {
    fn from(spec: timespec) -> Self {
        Timespec {
            high: spec.tv_sec,
            low: spec.tv_nsec,
        }
    }
}

#[cfg(target_family = "unix")]
impl Into<timespec> for Timespec {
    fn into(self) -> timespec {
        timespec {
            tv_sec: self.high,
            tv_nsec: self.low,
        }
    }
}

#[cfg(target_os = "windows")]
impl From<FILETIME> for Timespec {
    fn from(spec: FILETIME) -> Self {
        Timespec {
            high: spec.dwHighDateTime as i64,
            low: spec.dwLowDateTime as i64,
        }
    }
}

#[cfg(target_os = "windows")]
impl Into<FILETIME> for Timespec {
    fn into(self) -> FILETIME {
        FILETIME {
            dwHighDateTime: self.high as u32,
            dwLowDateTime: self.low as u32,
        }
    }
}

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
    pub options: Options,
}

bitflags! {
    #[derive(Serialize, Deserialize)]
    pub struct Options: u32 {
        const INITIAL_RSYNC      = 0b000001;
    }
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
        const RT_DSSC_ZLIB      = 0b000001;
        const RT_DSSC_CHUNKED   = 0b000010;
        const RT_DSSC_ZSTD      = 0b000100;
        const RT_MASK           = 0b000111;
        const STREAM_ZSTD       = 0b001000;
        const STREAM_LZ4        = 0b010000;
        const STREAM_MASK       = 0b011000;
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

pub fn zrle(buf: &mut [u8]) -> Option<&mut [u8]> {
    let mut saved: usize = 0;
    let mut i = 0;
    while i < buf.len() {
        let mut run: u8 = 0;
        while i < buf.len() && buf[i] == 0 && run != 255 {
            i += 1;
            run += 1;
        }
        if run == 1 && saved == 0 {
            return None;
        }
        saved = saved + run as usize - 2;
        eprintln!("{} {}", i, saved);
        buf[i - saved - 2] = 0;
        buf[i - saved - 1] = run;
        while i < buf.len() && buf[i] != 0 {
            buf[i - saved] = buf[i];
            i += 1;
        }
    }
    let final_len = buf.len() - saved;
    Some(&mut buf[..final_len])
}

#[test]
fn test_zrle() {
    let mut data = [0, 1, 0, 1, 0, 1, 2, 3, 0, 0];
    eprintln!("{:?}", data);
    eprintln!("{:?}", zrle(&mut data));
}

pub fn zrld(buf: &[u8], size_hint: Option<usize>) -> Vec<u8> {
    let mut vec = if let Some(size) = size_hint {
        Vec::with_capacity(size)
    } else {
        Vec::new()
    };
    let mut i = 0;
    while i < buf.len() {
        if buf[i] == 0 {
            i += 2;
            let num = buf[i - 1] as usize;
            unsafe { vec.set_len(vec.len() + num) };
            for j in vec.len() - num..vec.len() {
                vec[j] = 0;
            }
        }
        let old_i = i;
        while i < buf.len() && buf[i] != 0 {
            i += 1;
        }
        vec.extend_from_slice(&buf[old_i..i]);
    }
    vec
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
#[allow(non_camel_case_types)]
pub enum VFSCall<'a> {
    mknod {
        path: Cow<'a, Path>,
        mode: u32,
        rdev: u64,
        security: FileSecurity,
    },
    mkdir {
        path: Cow<'a, Path>,
        security: FileSecurity,
        mode: u32, // Attributes on windows
    },
    unlink {
        path: Cow<'a, Path>,
    },
    rmdir {
        path: Cow<'a, Path>,
    },
    symlink {
        from: Cow<'a, Path>,
        to: Cow<'a, Path>,
        security: FileSecurity,
    },
    rename {
        from: Cow<'a, Path>,
        to: Cow<'a, Path>,
        flags: u32,
    },
    link {
        from: Cow<'a, Path>,
        to: Cow<'a, Path>,
        security: FileSecurity,
    },
    chmod {
        path: Cow<'a, Path>,
        mode: u32,
    }, // On windows this represents attributes
    truncate {
        path: Cow<'a, Path>,
        size: i64,
    },
    write {
        path: Cow<'a, Path>,
        offset: i64,
        buf: Cow<'a, [u8]>,
    },
    diff_write {
        path: Cow<'a, Path>,
        offset: i64,
        buf: Cow<'a, [u8]>,
    },
    fallocate {
        path: Cow<'a, Path>,
        mode: i32,
        offset: i64,
        length: i64,
    },
    setxattr {
        path: Cow<'a, Path>,
        name: Cow<'a, CStr>,
        value: Cow<'a, [u8]>,
        flags: i32,
    },
    removexattr {
        path: Cow<'a, Path>,
        name: Cow<'a, CStr>,
    },
    create {
        path: Cow<'a, Path>,
        flags: i32,
        security: FileSecurity,
        mode: u32, // Attributes on windows
    },
    utimens {
        path: Cow<'a, Path>,
        timespec: [Timespec; 3], /* 2 on POSIX last is 0, 3 on Windows
                                  * (Created, Accessed, Written) */
    },
    fsync {
        path: Cow<'a, Path>,
        isdatasync: c_int,
    },
    truncating_write {
        path: Cow<'a, Path>,
        offset: i64,
        buf: Cow<'a, [u8]>,
        length: i64,
    },
    security {
        path: Cow<'a, Path>,
        security: FileSecurity,
    }, //chown on linux
}

pub fn translate_path(path: &Path, root: &Path) -> PathBuf {
    root.join(if path.starts_with("/") {
        path.strip_prefix("/").unwrap()
    } else {
        path
    })
}
metablock!(cfg(target_family = "unix") {
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

pub fn xor_buf(new: &mut [u8], old: &[u8]) {
    assert!(new.len() >= old.len());
    for i in 0..old.len() {
        new[i] ^= old[i];
    }
}
