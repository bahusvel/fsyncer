#![allow(non_camel_case_types)]
#![allow(private_in_public)]

extern crate crc;

mod bilog;
mod filestore;
mod forward;
mod store;
mod viewer;

pub use self::bilog::BilogEntry;
pub use self::crc::crc32;
pub use self::filestore::FileStore;
pub use self::store::*;
pub use self::viewer::viewer_main;

use common::*;
use errno::errno;
use error::{Error, FromError};
use libc::*;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::convert::TryFrom;
use std::fmt::Debug;
use std::fs::File;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use walkdir::{DirEntryExt, WalkDir};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum JournalType {
    Invalid,
    Bilog,
    Forward,
    Undo,
}

pub trait JournalEntry<'a>:
    Serialize + Deserialize<'a> + TryFrom<(&'a VFSCall<'a>, &'static Path)> + Clone
{
    fn journal(self, j: &mut Journal) -> Result<(), Error<io::Error>> {
        j.write_entry(self)
    }
    fn describe(&self, _: bool) -> String
    where
        Self: Debug,
    {
        format!("{:?}", self)
    }
    fn apply(&self, fspath: &Path) -> Result<VFSCall, Error<io::Error>>;
    fn affected_paths(&self) -> Vec<&Path>;
}

fn translate_and_stat(
    path: &Path,
    fspath: &Path,
) -> Result<stat, Error<io::Error>> {
    use std::mem;
    let real_path = translate_path(path, &fspath).into_cstring();
    let mut stbuf: stat = unsafe { mem::zeroed() };
    if unsafe { lstat(real_path.as_ptr(), &mut stbuf as *mut _) } == -1 {
        trace!(Err(io::Error::from(errno())));
    }
    Ok(stbuf)
}

fn find_hardlink(
    ino: u64,
    intree: &Path,
) -> Result<Option<PathBuf>, Error<io::Error>> {
    for entry in WalkDir::new(intree) {
        let e = trace!(entry.map_err(|e| io::Error::new(ErrorKind::Other, e)));
        if e.ino() == ino {
            return Ok(Some(e.path().to_path_buf()));
        }
    }
    return Ok(None);
}
