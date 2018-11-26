#![allow(non_camel_case_types)]
use common::*;
use errno::errno;
use libc::*;
use std::ffi::CString;
use std::fs;
use std::fs::File;
use std::io::{Error, ErrorKind};
use std::os::unix::io::FromRawFd;
use walkdir::DirEntryExt;

macro_rules! is_variant {
    ($val:expr, $variant:path) => {
        if let $variant(_) = $val {
            true
        } else {
            false
        }
    };
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

pub trait LogItem {
    fn current_state(&self, fspath: &str) -> Result<Self, Error>
    where
        Self: Sized;
    fn gen_bilog(oldstate: Self, newstate: Self) -> Self;
}

//Auto transforms
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct log_chmod(chmod<'static>);

impl<'a, 'b> From<&'b chmod<'a>> for log_chmod {
    fn from(c: &chmod) -> Self {
        log_chmod(chmod {
            path: Cow::Owned(c.path.clone().into_owned()),
            mode: c.mode,
        })
    }
}

impl LogItem for log_chmod {
    fn current_state(&self, fspath: &str) -> Result<Self, Error> {
        let stbuf = translate_and_stat(&self.0.path, fspath)?;
        Ok(log_chmod(chmod {
            path: self.0.path.clone(),
            mode: stbuf.st_mode,
        }))
    }
    fn gen_bilog(oldstate: Self, newstate: Self) -> Self {
        log_chmod(chmod {
            path: newstate.0.path,
            mode: newstate.0.mode ^ oldstate.0.mode,
        })
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct log_chown(chown<'static>);

impl<'a, 'b> From<&'b chown<'a>> for log_chown {
    fn from(c: &chown) -> Self {
        log_chown(chown {
            path: Cow::Owned(c.path.clone().into_owned()),
            uid: c.uid,
            gid: c.gid,
        })
    }
}

impl LogItem for log_chown {
    fn current_state(&self, fspath: &str) -> Result<Self, Error> {
        let stbuf = translate_and_stat(&self.0.path, fspath)?;
        Ok(log_chown(chown {
            path: self.0.path.clone(),
            uid: stbuf.st_uid,
            gid: stbuf.st_gid,
        }))
    }
    fn gen_bilog(oldstate: Self, newstate: Self) -> Self {
        log_chown(chown {
            path: newstate.0.path,
            uid: newstate.0.uid ^ oldstate.0.uid,
            gid: newstate.0.gid ^ newstate.0.gid,
        })
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct log_utimens(utimens<'static>);

impl<'a, 'b> From<&'b utimens<'a>> for log_utimens {
    fn from(u: &utimens) -> Self {
        log_utimens(utimens {
            path: Cow::Owned(u.path.clone().into_owned()),
            timespec: u.timespec.clone(),
        })
    }
}

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
    fn gen_bilog(oldstate: Self, newstate: Self) -> Self {
        log_utimens(utimens {
            path: newstate.0.path,
            timespec: [
                enc_timespec {
                    tv_sec: newstate.0.timespec[0].tv_sec ^ oldstate.0.timespec[0].tv_sec,
                    tv_nsec: newstate.0.timespec[0].tv_nsec ^ oldstate.0.timespec[0].tv_nsec,
                },
                enc_timespec {
                    tv_sec: newstate.0.timespec[1].tv_sec ^ oldstate.0.timespec[1].tv_sec,
                    tv_nsec: newstate.0.timespec[1].tv_nsec ^ oldstate.0.timespec[1].tv_nsec,
                },
            ],
        })
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct log_rename(rename<'static>);

impl<'a, 'b> From<&'b rename<'a>> for log_rename {
    fn from(r: &rename) -> Self {
        log_rename(rename {
            from: Cow::Owned(r.from.clone().into_owned()),
            to: Cow::Owned(r.to.clone().into_owned()),
            flags: r.flags,
        })
    }
}

impl LogItem for log_rename {
    fn current_state(&self, _fspath: &str) -> Result<Self, Error> {
        Ok(self.clone())
    }
    fn gen_bilog(_oldstate: Self, newstate: Self) -> Self {
        newstate
    }
}

//Trivial transforms
/*
    If directory exists it shall be removed with rmdir on the specified path, if the directory does not exist it shall be created on the specified path with the specified mode.
*/

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct log_dir(mkdir<'static>);

impl<'a, 'b> From<&'b mkdir<'a>> for log_dir {
    fn from(m: &mkdir) -> Self {
        log_dir(mkdir {
            path: Cow::Owned(m.path.clone().into_owned()),
            mode: m.mode,
        })
    }
}

impl<'a, 'b> From<&'b rmdir<'a>> for log_dir {
    fn from(r: &rmdir) -> Self {
        log_dir(mkdir {
            path: Cow::Owned(r.path.clone().into_owned()),
            mode: 0,
        })
    }
}

impl LogItem for log_dir {
    fn current_state(&self, fspath: &str) -> Result<Self, Error> {
        if self.0.mode != 0 {
            return Ok(self.clone());
        }
        let stbuf = translate_and_stat(&self.0.path, fspath)?;
        Ok(log_dir(mkdir {
            path: self.0.path.clone(),
            mode: stbuf.st_mode,
        }))
    }
    fn gen_bilog(oldstate: Self, _newstate: Self) -> Self {
        oldstate
    }
}

// Group transforms
/*
    If the file exists it's type needs to be identified, one of 4 below, and it is to be removed via unlink. If the file doesnt exist it is to be created from the type recorded with the specified parameters.
*/
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub enum log_file {
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

impl<'a, 'b> From<&'b symlink<'a>> for log_file {
    fn from(s: &symlink) -> Self {
        log_file::symlink(symlink {
            from: Cow::Owned(s.from.clone().into_owned()),
            to: Cow::Owned(s.to.clone().into_owned()),
        })
    }
}

impl<'a, 'b> From<&'b link<'a>> for log_file {
    fn from(l: &link) -> Self {
        log_file::link(link {
            from: Cow::Owned(l.from.clone().into_owned()),
            to: Cow::Owned(l.to.clone().into_owned()),
        })
    }
}

impl<'a, 'b> From<&'b mknod<'a>> for log_file {
    fn from(m: &mknod) -> Self {
        log_file::node(mknod {
            path: Cow::Owned(m.path.clone().into_owned()),
            mode: m.mode,
            rdev: m.rdev,
        })
    }
}

impl<'a, 'b> From<&'b create<'a>> for log_file {
    fn from(c: &create) -> Self {
        log_file::file(create {
            path: Cow::Owned(c.path.clone().into_owned()),
            mode: c.mode,
            flags: c.flags,
        })
    }
}

impl<'a, 'b> From<&'b unlink<'a>> for log_file {
    fn from(u: &unlink) -> Self {
        log_file::unlink(unlink {
            path: Cow::Owned(u.path.clone().into_owned()),
        })
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
            Err(ref err) if err.kind() == io::ErrorKind::NotFound => Ok(log_file::unlink(unlink {
                path: Cow::Owned(CString::from(self.file_path())),
            })), //file does not exist, creating
            Err(err) => Err(err),
        }
    }
    fn gen_bilog(oldstate: Self, newstate: Self) -> Self {
        if is_variant!(newstate, log_file::unlink) {
            oldstate
        } else {
            newstate
        }
    }
}

/*
If the attribute exists and its value matches that of recorded below it is to be removed, if the attribute doesn't exist or its value doesnt match the one below it is to be set.
*/
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct log_xattr {
    path: CString,
    name: CString,
    value: Option<Vec<u8>>,
}

impl<'a, 'b> From<&'b setxattr<'a>> for log_xattr {
    fn from(s: &setxattr) -> Self {
        log_xattr {
            path: s.path.clone().into_owned(),
            name: s.name.clone().into_owned(),
            value: Some(s.value.clone().into_owned()),
        }
    }
}

impl<'a, 'b> From<&'b removexattr<'a>> for log_xattr {
    fn from(r: &removexattr) -> Self {
        log_xattr {
            path: r.path.clone().into_owned(),
            name: r.name.clone().into_owned(),
            value: None,
        }
    }
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
        if len as i32 == ENOATTR {
            // new attribute is set
            return Ok(log_xattr {
                path: self.path.clone(),
                name: self.name.clone(),
                value: Some(
                    self.value
                        .as_ref()
                        .expect("No attribute is set, must be setting it")
                        .clone(),
                ),
            });
        }
        if len == -1 {
            return Err(Error::from(errno()));
        }
        // Removing or replacing the value
        Ok(log_xattr {
            path: self.path.clone(),
            name: self.name.clone(),
            value: Some(Vec::from(&val_buf[..len as usize])),
        })
    }
    fn gen_bilog(oldstate: Self, newstate: Self) -> Self {
        log_xattr {
            path: newstate.path,
            name: newstate.name,
            value: Some(if newstate.value.is_none() {
                oldstate.value.unwrap()
            } else {
                xor_largest_buf(newstate.value.unwrap(), oldstate.value.unwrap())
            }),
        }
    }
}

/*
    If the write offset + size > file length its reverse operation is to truncate offset + size - file length bytes of the end of the file, and write old data for the rest as follows. When overwriting old data the reverse is to write the old data.

    If the operation is truncate and removes part of the file its reverse operation is to write the missing data back in.
*/
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct log_write {
    path: CString,
    offset: int64_t,
    size: int64_t,
    buf: Vec<u8>,
}

fn xor_buf(new: &mut Vec<u8>, old: &Vec<u8>) {
    assert!(new.len() >= old.len());
    for i in 0..old.len() {
        new[i] ^= old[i];
    }
}

fn xor_largest_buf(mut new: Vec<u8>, mut old: Vec<u8>) -> Vec<u8> {
    if new.len() >= old.len() {
        xor_buf(&mut new, &old);
        new
    } else {
        xor_buf(&mut old, &new);
        old
    }
}

impl LogItem for log_write {
    fn current_state(&self, fspath: &str) -> Result<Self, Error> {
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
            return Err(io::Error::from(errno()));
        }

        Ok(current)
    }
    fn gen_bilog(oldstate: Self, mut newstate: Self) -> Self {
        xor_buf(&mut newstate.buf, &oldstate.buf);
        log_write {
            path: newstate.path,
            offset: newstate.offset,
            size: oldstate.size ^ newstate.size,
            buf: newstate.buf,
        }
    }
}
impl<'a, 'b> From<&'b write<'a>> for log_write {
    fn from(w: &write) -> Self {
        log_write {
            path: w.path.clone().into_owned(),
            offset: w.offset,
            size: w.offset + w.buf.len() as i64,
            buf: w.buf.clone().into_owned(),
        }
    }
}
impl<'a, 'b> From<&'b truncate<'a>> for log_write {
    fn from(t: &truncate) -> Self {
        log_write {
            path: t.path.clone().into_owned(),
            offset: 0,
            size: t.size,
            buf: Vec::new(),
        }
    }
}
