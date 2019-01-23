#![allow(non_camel_case_types)]
#![allow(private_in_public)]

mod bilog;
mod filestore;
mod store;
mod viewer;

pub use self::bilog::BilogEntry;
pub use self::store::*;
pub use self::viewer::viewer_main;
pub use self::filestore::FileStore;

use common::*;
use errno::errno;
use libc::*;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::{Error, ErrorKind};
use walkdir::{DirEntryExt, WalkDir};

pub trait JournalEntry<'a>: Serialize + Deserialize<'a> + Clone {
    fn from_vfscall(call: &VFSCall, fspath: &str) -> Result<Self, Error>;
    fn describe(&self, detail: bool) -> String;
    fn apply(&self, fspath: &str) -> Result<VFSCall, Error>;
    fn affected_paths(&self) -> Vec<&CStr>;
}

fn translate_and_stat(path: &CStr, fspath: &str) -> Result<stat, Error> {
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

fn find_hardlink(ino: u64, intree: &str) -> Result<Option<String>, Error> {
    for entry in WalkDir::new(intree) {
        let e = entry?;
        if e.ino() == ino {
            return Ok(Some(String::from(e.path().to_str().unwrap())));
        }
    }
    return Ok(None);
}
