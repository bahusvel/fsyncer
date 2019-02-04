#![allow(non_camel_case_types)]
#![allow(private_in_public)]

extern crate crc;

mod bilog;
mod filestore;
mod store;
mod viewer;

pub use self::bilog::BilogEntry;
pub use self::crc::crc32;
pub use self::filestore::FileStore;
pub use self::store::*;
pub use self::viewer::viewer_main;

use common::ffi::*;
use common::*;
use errno::errno;
use libc::*;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::ffi::CStr;
use std::fs::File;
use std::io::{Error, ErrorKind};
use walkdir::{DirEntryExt, WalkDir};

pub trait JournalEntry<'a>: Serialize + Deserialize<'a> + Clone {
    fn journal(self, j: &mut Journal) -> Result<(), Error>;
    fn from_vfscall(call: &VFSCall, fspath: &Path) -> Result<Self, Error>;
    fn describe(&self, detail: bool) -> String;
    fn apply(&self, fspath: &Path) -> Result<VFSCall, Error>;
    fn affected_paths(&self) -> Vec<&Path>;
}

fn translate_and_stat(path: &CStr, fspath: &Path) -> Result<stat, Error> {
    use std::mem;
    let real_path = translate_path(path, &fspath);
    let mut stbuf = unsafe {
        mem::transmute::<[u8; mem::size_of::<stat>()], stat>([0; mem::size_of::<stat>()])
    };
    if unsafe { lstat(real_path.as_ptr(), &mut stbuf as *mut _) } == -1 {
        return Err(Error::from(errno()));
    }
    Ok(stbuf)
}

fn find_hardlink(ino: u64, intree: &Path) -> Result<Option<PathBuf>, Error> {
    for entry in WalkDir::new(intree) {
        let e = entry?;
        if e.ino() == ino {
            return Ok(Some(e.path().to_path_buf()));
        }
    }
    return Ok(None);
}
