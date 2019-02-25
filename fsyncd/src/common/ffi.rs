pub use std::ffi::{CStr, CString};
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
pub use std::path::{Path, PathBuf};

pub trait ToPath {
    fn to_path(&self) -> &Path;
}

impl ToPath for CStr {
    #[inline(always)]
    fn to_path(&self) -> &Path {
        Path::new(OsStr::from_bytes(self.to_bytes()))
    }
}

pub trait ToPathBuf {
    fn into_pathbuf(self) -> PathBuf;
}

impl ToPathBuf for CString {
    #[inline(always)]
    fn into_pathbuf(self) -> PathBuf {
        PathBuf::from(OsString::from_vec(self.into_bytes()))
    }
}

pub trait ToCString {
    fn into_cstring(self) -> CString;
}
impl ToCString for PathBuf {
    #[inline(always)]
    fn into_cstring(self) -> CString {
        CString::new(self.into_os_string().into_vec()).unwrap()
    }
}
