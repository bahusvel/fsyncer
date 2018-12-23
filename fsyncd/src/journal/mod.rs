#![allow(non_camel_case_types)]
#![allow(private_in_public)]

mod bilog;
mod store;
mod viewer;

pub use self::store::*;
pub use self::viewer::viewer_main;

pub use self::bilog::BilogItem;
use common::*;
use errno::errno;
use libc::*;
use std::borrow::Cow;
use std::ffi::{CStr, CString};
use std::fs::{self, File};
use std::io::{Error, ErrorKind};
use std::os::unix::io::FromRawFd;
use walkdir::{DirEntryExt, WalkDir};

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub enum JournalCall {
    log_chmod(log_chmod),
    log_chown(log_chown),
    log_utimens(log_utimens),
    log_rename(log_rename),
    log_dir(log_dir),
    log_file(log_file),
    log_xattr(log_xattr),
    log_write(log_write),
}

impl<'a, 'b> From<&'b VFSCall<'a>> for JournalCall {
    fn from(call: &VFSCall) -> JournalCall {
        match call {
            VFSCall::mknod(m) => JournalCall::log_file(log_file::node(mknod {
                path: Cow::Owned(m.path.clone().into_owned()),
                mode: m.mode,
                rdev: m.rdev,
            })),
            VFSCall::mkdir(m) => JournalCall::log_dir(log_dir {
                path: m.path.clone().into_owned(),
                mode: m.mode,
                dir_exists: false,
            }),
            VFSCall::unlink(u) => JournalCall::log_file(log_file::unlink(unlink {
                path: Cow::Owned(u.path.clone().into_owned()),
            })),
            VFSCall::rmdir(r) => JournalCall::log_dir(log_dir {
                path: r.path.clone().into_owned(),
                mode: 0,
                dir_exists: true,
            }),
            VFSCall::symlink(s) => JournalCall::log_file(log_file::symlink(symlink {
                from: Cow::Owned(s.from.clone().into_owned()),
                to: Cow::Owned(s.to.clone().into_owned()),
            })),
            VFSCall::rename(r) => JournalCall::log_rename(log_rename {
                from: r.from.clone().into_owned(),
                to: r.to.clone().into_owned(),
                from_exists: true,
            }),
            VFSCall::link(l) => JournalCall::log_file(log_file::link(link {
                from: Cow::Owned(l.from.clone().into_owned()),
                to: Cow::Owned(l.to.clone().into_owned()),
            })),
            VFSCall::chmod(c) => JournalCall::log_chmod(log_chmod(chmod {
                path: Cow::Owned(c.path.clone().into_owned()),
                mode: c.mode,
            })),
            VFSCall::chown(c) => JournalCall::log_chown(log_chown(chown {
                path: Cow::Owned(c.path.clone().into_owned()),
                uid: c.uid,
                gid: c.gid,
            })),
            VFSCall::truncate(t) => JournalCall::log_write(log_write {
                path: t.path.clone().into_owned(),
                offset: 0,
                size: t.size,
                buf: Vec::new(),
            }),
            VFSCall::write(w) => JournalCall::log_write(log_write {
                path: w.path.clone().into_owned(),
                offset: w.offset,
                size: w.offset + w.buf.len() as i64,
                buf: w.buf.clone().into_owned(),
            }),
            VFSCall::setxattr(s) => JournalCall::log_xattr(log_xattr {
                path: s.path.clone().into_owned(),
                name: s.name.clone().into_owned(),
                value: Some(s.value.clone().into_owned()),
            }),
            VFSCall::removexattr(r) => JournalCall::log_xattr(log_xattr {
                path: r.path.clone().into_owned(),
                name: r.name.clone().into_owned(),
                value: None,
            }),
            VFSCall::create(c) => JournalCall::log_file(log_file::file(create {
                path: Cow::Owned(c.path.clone().into_owned()),
                mode: c.mode,
                flags: c.flags,
            })),
            VFSCall::utimens(u) => JournalCall::log_utimens(log_utimens(utimens {
                path: Cow::Owned(u.path.clone().into_owned()),
                timespec: u.timespec.clone(),
            })),
            VFSCall::fallocate(_fallocate) => panic!("Not implemented"),
            VFSCall::fsync(_fsync) => panic!("Not an IO call"),
        }
    }
}

impl LogItem for JournalCall {
    fn current_state(&self, fspath: &str) -> Result<Self, Error> {
        Ok(match self {
            JournalCall::log_chmod(c) => JournalCall::log_chmod(c.current_state(fspath)?),
            JournalCall::log_chown(c) => JournalCall::log_chown(c.current_state(fspath)?),
            JournalCall::log_utimens(c) => JournalCall::log_utimens(c.current_state(fspath)?),
            JournalCall::log_rename(c) => JournalCall::log_rename(c.current_state(fspath)?),
            JournalCall::log_dir(c) => JournalCall::log_dir(c.current_state(fspath)?),
            JournalCall::log_file(c) => JournalCall::log_file(c.current_state(fspath)?),
            JournalCall::log_xattr(c) => JournalCall::log_xattr(c.current_state(fspath)?),
            JournalCall::log_write(c) => JournalCall::log_write(c.current_state(fspath)?),
        })
    }
    fn affected_paths(&self) -> Vec<&CStr> {
        match self {
            JournalCall::log_chmod(c) => c.affected_paths(),
            JournalCall::log_chown(c) => c.affected_paths(),
            JournalCall::log_utimens(c) => c.affected_paths(),
            JournalCall::log_rename(c) => c.affected_paths(),
            JournalCall::log_dir(c) => c.affected_paths(),
            JournalCall::log_file(c) => c.affected_paths(),
            JournalCall::log_xattr(c) => c.affected_paths(),
            JournalCall::log_write(c) => c.affected_paths(),
        }
    }
}

pub trait LogItem {
    fn current_state(&self, fspath: &str) -> Result<Self, Error>
    where
        Self: Sized;
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

//Auto transforms
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
struct log_chmod(chmod<'static>);

impl LogItem for log_chmod {
    fn current_state(&self, fspath: &str) -> Result<Self, Error> {
        let stbuf = translate_and_stat(&self.0.path, fspath)?;
        Ok(log_chmod(chmod {
            path: self.0.path.clone(),
            mode: stbuf.st_mode,
        }))
    }
    fn affected_paths(&self) -> Vec<&CStr> {
        vec![&*self.0.path]
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
struct log_chown(chown<'static>);

impl LogItem for log_chown {
    fn current_state(&self, fspath: &str) -> Result<Self, Error> {
        let stbuf = translate_and_stat(&self.0.path, fspath)?;
        Ok(log_chown(chown {
            path: self.0.path.clone(),
            uid: stbuf.st_uid,
            gid: stbuf.st_gid,
        }))
    }
    fn affected_paths(&self) -> Vec<&CStr> {
        vec![&*self.0.path]
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
struct log_utimens(utimens<'static>);

impl LogItem for log_utimens {
    fn current_state(&self, fspath: &str) -> Result<Self, Error> {
        let stbuf = translate_and_stat(&self.0.path, fspath)?;
        Ok(log_utimens(utimens {
            path: self.0.path.clone(),
            timespec: [
                enc_timespec {
                    tv_sec: stbuf.st_atime,
                    tv_nsec: stbuf.st_atime_nsec,
                },
                enc_timespec {
                    tv_sec: stbuf.st_mtime,
                    tv_nsec: stbuf.st_mtime_nsec,
                },
            ],
        }))
    }
    fn affected_paths(&self) -> Vec<&CStr> {
        vec![&*self.0.path]
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
struct log_rename {
    from: CString,
    to: CString,
    from_exists: bool,
}

impl LogItem for log_rename {
    fn current_state(&self, _fspath: &str) -> Result<Self, Error> {
        Ok(self.clone())
    }
    fn affected_paths(&self) -> Vec<&CStr> {
        vec![&self.from, &self.to]
    }
}

//Trivial transforms
/*
    If directory exists it shall be removed with rmdir on the specified path, if the directory does not exist it shall be created on the specified path with the specified mode.
*/

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
struct log_dir {
    path: CString,
    mode: uint32_t,
    dir_exists: bool,
}

impl LogItem for log_dir {
    fn current_state(&self, fspath: &str) -> Result<Self, Error> {
        if self.mode != 0 {
            return Ok(self.clone());
        }
        let stbuf = translate_and_stat(&self.path, fspath)?;
        Ok(log_dir {
            path: self.path.clone(),
            mode: stbuf.st_mode,
            dir_exists: true,
        })
    }
    fn affected_paths(&self) -> Vec<&CStr> {
        vec![&*self.path]
    }
}

// Group transforms
/*
    If the file exists it's type needs to be identified, one of 4 below, and it is to be removed via unlink. If the file doesnt exist it is to be created from the type recorded with the specified parameters.
*/
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
enum log_file {
    symlink(symlink<'static>),
    link(link<'static>),
    node(mknod<'static>),
    file(create<'static>),
    unlink(unlink<'static>),
}

impl log_file {
    #[inline]
    fn file_path(&self) -> &CStr {
        match self {
            log_file::symlink(symlink { to, .. }) => to,
            log_file::link(link { to, .. }) => to,
            log_file::node(mknod { path, .. }) => path,
            log_file::file(create { path, .. }) => path,
            log_file::unlink(unlink { path }) => path,
        }
    }
}

impl LogItem for log_file {
    fn current_state(&self, fspath: &str) -> Result<Self, Error> {
        #[inline(always)]
        fn is_type(mode: uint32_t, ftype: uint32_t) -> bool {
            mode & ftype == ftype
        }
        let stat_res = translate_and_stat(&self.file_path(), fspath);
        match stat_res {
            Ok(stbuf) => {
                assert!(is_variant!(self, log_file::unlink));
                let m = stbuf.st_mode;
                // file exists, deleting
                if is_type(m, S_IFREG) {
                    // Regular file
                    if stbuf.st_nlink > 1 {
                        println!("This file is hard linked, balls...");
                        let dst = find_hardlink(stbuf.st_ino, fspath)?;
                        if dst.is_none() {
                            return Err(Error::new(
                                ErrorKind::Other,
                                "File is hardlinked outside fsyncer path",
                            ));
                        }
                        Ok(log_file::link(link {
                            from: Cow::Owned(CString::new(dst.unwrap()).unwrap()),
                            to: Cow::Owned(CString::from(self.file_path())),
                        }))
                    } else {
                        Ok(log_file::file(create {
                            path: Cow::Owned(CString::from(self.file_path())),
                            mode: stbuf.st_mode,
                            flags: 0,
                        }))
                    }
                } else if is_type(m, S_IFLNK) {
                    // symlink
                    let real_path = translate_path(&self.file_path(), &fspath);
                    let dst = fs::read_link(
                        real_path
                            .to_str()
                            .map_err(|e| Error::new(ErrorKind::Other, e))?,
                    )?;

                    Ok(log_file::symlink(symlink {
                        from: Cow::Owned(CString::new(dst.to_str().unwrap()).unwrap()),
                        to: Cow::Owned(CString::from(self.file_path())),
                    }))
                } else if is_type(m, S_IFBLK)
                    || is_type(m, S_IFCHR)
                    || is_type(m, S_IFIFO)
                    || is_type(m, S_IFSOCK)
                //http://man7.org/linux/man-pages/man2/mknod.2.html
                {
                    Ok(log_file::node(mknod {
                        path: Cow::Owned(CString::from(self.file_path())),
                        mode: stbuf.st_mode,
                        rdev: stbuf.st_rdev,
                    }))
                } else {
                    panic!("log_file does not understand {:x} file type", m & S_IFMT);
                }
            }
            Err(ref err) if err.kind() == ErrorKind::NotFound => Ok(log_file::unlink(unlink {
                path: Cow::Owned(CString::from(self.file_path())),
            })), //file does not exist, creating
            Err(err) => Err(err),
        }
    }
    fn affected_paths(&self) -> Vec<&CStr> {
        vec![self.file_path()]
    }
}

/*
If the attribute exists and its value matches that of recorded below it is to be removed, if the attribute doesn't exist or its value doesnt match the one below it is to be set.
*/
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
struct log_xattr {
    path: CString,
    name: CString,
    value: Option<Vec<u8>>,
}

impl LogItem for log_xattr {
    fn current_state(&self, fspath: &str) -> Result<Self, Error> {
        let real_path = translate_path(&self.path, &fspath);
        let mut val_buf: [u8; 4096] = [0; 4096];
        let len = unsafe {
            lgetxattr(
                real_path.as_ptr(),
                self.name.as_ptr(),
                val_buf.as_mut_ptr() as *mut _,
                4096, // HACK, I should query the size first
            )
        };
        if len == -1 {
            let err = errno();
            let interr: i32 = err.into();
            if interr == ENOATTR {
                return Ok(log_xattr {
                    path: self.path.clone(),
                    name: self.name.clone(),
                    value: Some(
                        self.value
                            .as_ref()
                            .expect("Attribute is not set, must be setting it")
                            .clone(),
                    ),
                });
            }
            return Err(Error::from(err));
        }
        // Removing or replacing the value
        Ok(log_xattr {
            path: self.path.clone(),
            name: self.name.clone(),
            value: Some(Vec::from(&val_buf[..len as usize])),
        })
    }
    fn affected_paths(&self) -> Vec<&CStr> {
        vec![&self.path]
    }
}

/*
    If the write offset + size > file length its reverse operation is to truncate offset + size - file length bytes of the end of the file, and write old data for the rest as follows. When overwriting old data the reverse is to write the old data.

    If the operation is truncate and removes part of the file its reverse operation is to write the missing data back in.
*/
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
struct log_write {
    path: CString,
    offset: int64_t,
    size: int64_t,
    buf: Vec<u8>,
}

impl LogItem for log_write {
    fn current_state(&self, fspath: &str) -> Result<Self, Error> {
        //println!("Atleast here");
        let real_path = translate_path(self.path.as_c_str(), &fspath);
        let fd = unsafe { open(real_path.as_ptr(), O_RDONLY) };
        if fd == -1 {
            return Err(Error::from(errno()));
        }

        let f = unsafe { File::from_raw_fd(fd) };

        let mut current = log_write {
            path: self.path.clone(),
            offset: self.offset,
            size: f.metadata()?.len() as i64,
            buf: Vec::new(),
        };

        // There is no overlap, operation is appending write or truncate
        if self.offset == current.size {
            return Ok(current);
        }

        let overlap = self.size - current.size;
        current.buf.reserve(overlap as usize);
        unsafe { current.buf.set_len(overlap as usize) };

        let res = unsafe {
            pread(
                fd,
                current.buf.as_mut_ptr() as *mut _,
                overlap as usize,
                self.offset,
            )
        };
        if res == -1 {
            return Err(Error::from(errno()));
        }

        Ok(current)
    }
    fn affected_paths(&self) -> Vec<&CStr> {
        vec![&self.path]
    }
}
