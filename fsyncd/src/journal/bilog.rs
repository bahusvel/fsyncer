use common::*;
use journal::*;

use std::cmp::min;
use std::ffi::OsStr;
use std::io::Error;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::FileExt;

//  idealistic type to express these things
use std::marker::PhantomData;

pub trait BilogState {}
struct Old {}
impl BilogState for Old {}
struct New {}
impl BilogState for New {}
#[derive(Debug, Clone)]
struct Xor {}
impl BilogState for Xor {}
trait NewS {}
trait OldS {}
trait XorS {}

trait Bilog: XorS {
    type N: NewS;
    type O: OldS;
    type X: XorS;
    fn new(call: &VFSCall) -> Self::N;
    fn xor(o: &Self::O, n: &Self::N) -> Self::X;
    fn apply<'a>(x: &'a Self::X, o: &Self::O) -> VFSCall<'a>;
    fn old(new: Option<&Self::N>, xor: Option<&Self::X>, fspath: &str) -> Result<Self::O, Error>;
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum BilogEntry {
    chmod(bilog_chmod<Xor>),
    chown(bilog_chown<Xor>),
    utimens(bilog_utimens<Xor>),
    rename(bilog_rename<Xor>),
    dir(bilog_dir<Xor>),
    symlink(bilog_symlink<Xor>),
    link(bilog_link<Xor>),
    node(bilog_node<Xor>),
    file(bilog_file<Xor>),
    truncate(bilog_truncate<Xor>),
    write(bilog_write<Xor>),
    xattr(bilog_xattr<Xor>),
}

impl<'a> JournalEntry<'a> for BilogEntry {
    fn from_vfscall(call: &VFSCall, fspath: &str) -> Result<Self, Error> {
        Ok(match call {
            VFSCall::mknod(_) => {
                let new = bilog_node::new(call);
                let old = bilog_node::old(Some(&new), None, fspath)?;
                BilogEntry::node(bilog_node::xor(&old, &new))
            }
            VFSCall::mkdir(_) | VFSCall::rmdir(_) => {
                let new = bilog_dir::new(call);
                let old = bilog_dir::old(Some(&new), None, fspath)?;
                BilogEntry::dir(bilog_dir::xor(&old, &new))
            }
            VFSCall::unlink(u) => {
                #[inline(always)]
                fn is_type(mode: uint32_t, ftype: uint32_t) -> bool {
                    mode & ftype == ftype
                }
                let stbuf = translate_and_stat(&u.path, fspath)?;
                let m = stbuf.st_mode;

                if is_type(m, S_IFREG) {
                    if stbuf.st_nlink > 1 {
                        let new = bilog_link::new(call);
                        let old = bilog_link::old(Some(&new), None, fspath)?;
                        BilogEntry::link(bilog_link::xor(&old, &new))
                    } else {
                        let new = bilog_file::new(call);
                        let old = bilog_file::old(Some(&new), None, fspath)?;
                        BilogEntry::file(bilog_file::xor(&old, &new))
                    }
                } else if is_type(m, S_IFLNK) {
                    let new = bilog_symlink::new(call);
                    let old = bilog_symlink::old(Some(&new), None, fspath)?;
                    BilogEntry::symlink(bilog_symlink::xor(&old, &new))
                } else if is_type(m, S_IFBLK)
                    || is_type(m, S_IFCHR)
                    || is_type(m, S_IFIFO)
                    || is_type(m, S_IFSOCK)
                {
                    let new = bilog_node::new(call);
                    let old = bilog_node::old(Some(&new), None, fspath)?;
                    BilogEntry::node(bilog_node::xor(&old, &new))
                } else {
                    panic!("Unknown file type deleted");
                }
            }
            VFSCall::symlink(_) => {
                let new = bilog_symlink::new(call);
                let old = bilog_symlink::old(Some(&new), None, fspath)?;
                BilogEntry::symlink(bilog_symlink::xor(&old, &new))
            }
            VFSCall::rename(_) => {
                let new = bilog_rename::new(call);
                let old = bilog_rename::old(Some(&new), None, fspath)?;
                BilogEntry::rename(bilog_rename::xor(&old, &new))
            }
            VFSCall::link(_) => {
                let new = bilog_link::new(call);
                let old = bilog_link::old(Some(&new), None, fspath)?;
                BilogEntry::link(bilog_link::xor(&old, &new))
            }
            VFSCall::chmod(_) => {
                let new = bilog_chmod::new(call);
                let old = bilog_chmod::old(Some(&new), None, fspath)?;
                BilogEntry::chmod(bilog_chmod::xor(&old, &new))
            }
            VFSCall::chown(_) => {
                let new = bilog_chown::new(call);
                let old = bilog_chown::old(Some(&new), None, fspath)?;
                BilogEntry::chown(bilog_chown::xor(&old, &new))
            }
            VFSCall::truncate(_) => {
                let new = bilog_truncate::new(call);
                let old = bilog_truncate::old(Some(&new), None, fspath)?;
                BilogEntry::truncate(bilog_truncate::xor(&old, &new))
            }
            VFSCall::write(_) => {
                let new = bilog_write::new(call);
                let old = bilog_write::old(Some(&new), None, fspath)?;
                BilogEntry::write(bilog_write::xor(&old, &new))
            }
            VFSCall::setxattr(_) | VFSCall::removexattr(_) => {
                let new = bilog_xattr::new(call);
                let old = bilog_xattr::old(Some(&new), None, fspath)?;
                BilogEntry::xattr(bilog_xattr::xor(&old, &new))
            }
            VFSCall::create(_) => {
                let new = bilog_file::new(call);
                let old = bilog_file::old(Some(&new), None, fspath)?;
                BilogEntry::file(bilog_file::xor(&old, &new))
            }
            VFSCall::utimens(_) => {
                let new = bilog_utimens::new(call);
                let old = bilog_utimens::old(Some(&new), None, fspath)?;
                BilogEntry::utimens(bilog_utimens::xor(&old, &new))
            }
            VFSCall::fallocate(_) => panic!("Not implemented"),
            VFSCall::fsync(_) => panic!("Not an IO call"),
        })
    }
    fn describe(&self, detail: bool) -> String {
        if detail {
            return format!("{:?}", self);
        }
        match self {
            BilogEntry::chmod(c) => format!("{:?} changed permissions", c.path),
            BilogEntry::chown(c) => format!("{:?} changed onwership", c.path),
            BilogEntry::utimens(c) => format!("{:?} changed mtime/ctime", c.path),
            BilogEntry::rename(c) => format!("{:?} and {:?} exchanged names", c.from, c.to),
            BilogEntry::dir(c) => format!("{:?} created or removed directory", c.path),
            BilogEntry::symlink(c) => format!("{:?} created or removed symlink", c.to),
            BilogEntry::link(c) => format!("{:?} created or removed link", c.to),
            BilogEntry::node(c) => format!("{:?} created or removed special file", c.path),
            BilogEntry::file(c) => format!("{:?} created or removed normal file", c.path),
            BilogEntry::truncate(c) => format!("{:?} truncated or extended file", c.path),
            BilogEntry::write(c) => format!("{:?} changed contents at offset {}", c.path, c.offset),
            BilogEntry::xattr(c) => format!(
                "{:?} set, changed or removed extended attribute {:?}",
                c.path, c.name,
            ),
        }
    }
    fn affected_paths(&self) -> Vec<&CStr> {
        match self {
            BilogEntry::chmod(c) => vec![&c.path],
            BilogEntry::chown(c) => vec![&c.path],
            BilogEntry::utimens(c) => vec![&c.path],
            BilogEntry::rename(c) => vec![&c.from, &c.to],
            BilogEntry::dir(c) => vec![&c.path],
            BilogEntry::symlink(c) => vec![&c.to],
            BilogEntry::link(c) => vec![&c.to],
            BilogEntry::node(c) => vec![&c.path],
            BilogEntry::file(c) => vec![&c.path],
            BilogEntry::truncate(c) => vec![&c.path],
            BilogEntry::write(c) => vec![&c.path],
            BilogEntry::xattr(c) => vec![&c.path],
        }
    }
    fn apply(&self, fspath: &str) -> Result<VFSCall, Error> {
        match self {
            BilogEntry::chmod(x) => {
                let current = bilog_chmod::old(None, Some(x), fspath)?;
                Ok(bilog_chmod::apply(x, &current))
            }
            BilogEntry::chown(x) => {
                let current = bilog_chown::old(None, Some(x), fspath)?;
                Ok(bilog_chown::apply(x, &current))
            }
            BilogEntry::utimens(x) => {
                let current = bilog_utimens::old(None, Some(x), fspath)?;
                Ok(bilog_utimens::apply(x, &current))
            }
            BilogEntry::rename(x) => {
                let current = bilog_rename::old(None, Some(x), fspath)?;
                Ok(bilog_rename::apply(x, &current))
            }
            BilogEntry::dir(x) => {
                let current = bilog_dir::old(None, Some(x), fspath)?;
                Ok(bilog_dir::apply(x, &current))
            }
            BilogEntry::symlink(x) => {
                let current = bilog_symlink::old(None, Some(x), fspath)?;
                Ok(bilog_symlink::apply(x, &current))
            }
            BilogEntry::link(x) => {
                let current = bilog_link::old(None, Some(x), fspath)?;
                Ok(bilog_link::apply(x, &current))
            }
            BilogEntry::node(x) => {
                let current = bilog_node::old(None, Some(x), fspath)?;
                Ok(bilog_node::apply(x, &current))
            }
            BilogEntry::file(x) => {
                let current = bilog_file::old(None, Some(x), fspath)?;
                Ok(bilog_file::apply(x, &current))
            }
            BilogEntry::truncate(x) => {
                let current = bilog_truncate::old(None, Some(x), fspath)?;
                Ok(bilog_truncate::apply(x, &current))
            }
            BilogEntry::write(x) => {
                let current = bilog_write::old(None, Some(x), fspath)?;
                Ok(bilog_write::apply(x, &current))
            }
            BilogEntry::xattr(x) => {
                let current = bilog_xattr::old(None, Some(x), fspath)?;
                Ok(bilog_xattr::apply(x, &current))
            }
        }
    }
}

macro_rules! bilog_entry {
    ($name:ident {$($field:ident: $ft:ty,)*}) => {
        #[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
        pub struct $name<S: BilogState> {
            $(
                pub $field: $ft,
            )*
            s: PhantomData<S>,
        }
        impl NewS for $name<New> {}
        impl OldS for $name<Old> {}
        impl XorS for $name<Xor> {}
    };
    ($name:ident {$($field:ident: $ft:ty),*}) => {
        bilog_entry!($name { $($field: $ft,)*});
    }
}

macro_rules! path_bilog {
    ($name:ident {$($field:ident: $ft:ty),*}) => {
         bilog_entry!($name {path:  CString, $($field: $ft,)* });
    }
}

path_bilog!(bilog_chmod { mode: mode_t });
impl Bilog for bilog_chmod<Xor> {
    type N = bilog_chmod<New>;
    type O = bilog_chmod<Old>;
    type X = bilog_chmod<Xor>;
    fn new(call: &VFSCall) -> Self::N {
        if let VFSCall::chmod(c) = call {
            bilog_chmod {
                path: c.path.clone().into_owned(),
                mode: c.mode,
                s: PhantomData,
            }
        } else {
            panic!("Cannot generate from {:?}", call)
        }
    }
    fn xor(o: &Self::O, n: &Self::N) -> Self::X {
        bilog_chmod {
            path: n.path.clone(),
            mode: n.mode ^ o.mode,
            s: PhantomData,
        }
    }
    fn apply<'a>(x: &'a Self::X, o: &Self::O) -> VFSCall<'a> {
        VFSCall::chmod(chmod {
            path: Cow::Borrowed(&x.path),
            mode: x.mode ^ o.mode,
        })
    }
    fn old(new: Option<&Self::N>, xor: Option<&Self::X>, fspath: &str) -> Result<Self::O, Error> {
        let path = new
            .map(|n| n.path.clone())
            .unwrap_or(xor.map(|x| x.path.clone()).unwrap());
        let stbuf = translate_and_stat(&path, fspath)?;
        Ok(bilog_chmod {
            path: path,
            mode: stbuf.st_mode,
            s: PhantomData,
        })
    }
}
path_bilog!(bilog_chown {
    uid: uint32_t,
    gid: uint32_t
});
impl Bilog for bilog_chown<Xor> {
    type N = bilog_chown<New>;
    type O = bilog_chown<Old>;
    type X = bilog_chown<Xor>;
    fn new(call: &VFSCall) -> Self::N {
        if let VFSCall::chown(c) = call {
            bilog_chown {
                path: c.path.clone().into_owned(),
                uid: c.uid,
                gid: c.gid,
                s: PhantomData,
            }
        } else {
            panic!("Cannot generate from {:?}", call)
        }
    }
    fn xor(o: &Self::O, n: &Self::N) -> Self::X {
        bilog_chown {
            path: n.path.clone(),
            uid: n.uid ^ o.uid,
            gid: n.gid ^ o.gid,
            s: PhantomData,
        }
    }
    fn apply<'a>(x: &'a Self::X, o: &Self::O) -> VFSCall<'a> {
        VFSCall::chown(chown {
            path: Cow::Borrowed(&x.path),
            uid: x.uid ^ o.uid,
            gid: x.gid ^ o.gid,
        })
    }
    fn old(new: Option<&Self::N>, xor: Option<&Self::X>, fspath: &str) -> Result<Self::O, Error> {
        let path = new
            .map(|n| n.path.clone())
            .unwrap_or(xor.map(|x| x.path.clone()).unwrap());
        let stbuf = translate_and_stat(&path, fspath)?;
        Ok(bilog_chown {
            path: path,
            uid: stbuf.st_uid,
            gid: stbuf.st_gid,
            s: PhantomData,
        })
    }
}
path_bilog!(bilog_utimens {
    timespec: [enc_timespec; 2]
});
impl Bilog for bilog_utimens<Xor> {
    type N = bilog_utimens<New>;
    type O = bilog_utimens<Old>;
    type X = bilog_utimens<Xor>;
    fn new(call: &VFSCall) -> Self::N {
        if let VFSCall::utimens(c) = call {
            bilog_utimens {
                path: c.path.clone().into_owned(),
                timespec: c.timespec.clone(),
                s: PhantomData,
            }
        } else {
            panic!("Cannot generate from {:?}", call)
        }
    }
    fn xor(o: &Self::O, n: &Self::N) -> Self::X {
        bilog_utimens {
            path: n.path.clone(),
            timespec: [
                n.timespec[0].xor(&o.timespec[0]),
                n.timespec[1].xor(&o.timespec[1]),
            ],
            s: PhantomData,
        }
    }
    fn apply<'a>(x: &'a Self::X, o: &Self::O) -> VFSCall<'a> {
        VFSCall::utimens(utimens {
            path: Cow::Borrowed(&x.path),
            timespec: [
                x.timespec[0].xor(&o.timespec[0]),
                x.timespec[1].xor(&o.timespec[1]),
            ],
        })
    }
    fn old(new: Option<&Self::N>, xor: Option<&Self::X>, fspath: &str) -> Result<Self::O, Error> {
        let path = new
            .map(|n| n.path.clone())
            .unwrap_or(xor.map(|x| x.path.clone()).unwrap());
        let stbuf = translate_and_stat(&path, fspath)?;
        Ok(bilog_utimens {
            path: path,
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
            s: PhantomData,
        })
    }
}
bilog_entry!(bilog_rename {
    from: CString,
    to: CString,
    from_exists: bool
});
impl Bilog for bilog_rename<Xor> {
    type N = bilog_rename<New>;
    type O = bilog_rename<Old>;
    type X = bilog_rename<Xor>;
    fn new(call: &VFSCall) -> Self::N {
        if let VFSCall::rename(c) = call {
            bilog_rename {
                from: c.from.clone().into_owned(),
                to: c.to.clone().into_owned(),
                from_exists: true,
                s: PhantomData,
            }
        } else {
            panic!("Cannot generate from {:?}", call)
        }
    }
    fn xor(o: &Self::O, _: &Self::N) -> Self::X {
        bilog_rename {
            from: o.from.clone(),
            to: o.to.clone(),
            from_exists: true,
            s: PhantomData,
        }
    }
    fn apply<'a>(x: &'a Self::X, o: &Self::O) -> VFSCall<'a> {
        if o.from_exists {
            VFSCall::rename(rename {
                from: Cow::Borrowed(&x.from),
                to: Cow::Borrowed(&x.to),
                flags: 0,
            })
        } else {
            VFSCall::rename(rename {
                from: Cow::Borrowed(&x.to),
                to: Cow::Borrowed(&x.from),
                flags: 0,
            })
        }
    }
    fn old(new: Option<&Self::N>, xor: Option<&Self::X>, _: &str) -> Result<Self::O, Error> {
        let from = new
            .map(|n| n.from.clone())
            .unwrap_or(xor.map(|x| x.from.clone()).unwrap());
        let to = new
            .map(|n| n.to.clone())
            .unwrap_or(xor.map(|x| x.to.clone()).unwrap());
        Ok(bilog_rename {
            from,
            to,
            from_exists: true,
            s: PhantomData,
        })
    }
}
path_bilog!(bilog_dir {
    mode: uint32_t,
    dir_exists: bool
});
impl Bilog for bilog_dir<Xor> {
    type N = bilog_dir<New>;
    type O = bilog_dir<Old>;
    type X = bilog_dir<Xor>;
    fn new(call: &VFSCall) -> Self::N {
        match call {
            VFSCall::rmdir(r) => bilog_dir {
                path: r.path.clone().into_owned(),
                mode: 0,
                dir_exists: true,
                s: PhantomData,
            },
            VFSCall::mkdir(m) => bilog_dir {
                path: m.path.clone().into_owned(),
                mode: m.mode,
                dir_exists: false,
                s: PhantomData,
            },
            _ => panic!("Cannot generate from {:?}", call),
        }
    }
    fn xor(o: &Self::O, n: &Self::N) -> Self::X {
        bilog_dir {
            path: o.path.clone(),
            mode: if n.dir_exists { o.mode } else { n.mode },
            dir_exists: true,
            s: PhantomData,
        }
    }
    fn apply<'a>(x: &'a Self::X, o: &Self::O) -> VFSCall<'a> {
        if o.dir_exists {
            VFSCall::rmdir(rmdir {
                path: Cow::Borrowed(&x.path),
            })
        } else {
            VFSCall::mkdir(mkdir {
                path: Cow::Borrowed(&x.path),
                mode: x.mode,
            })
        }
    }
    fn old(new: Option<&Self::N>, xor: Option<&Self::X>, fspath: &str) -> Result<Self::O, Error> {
        let path = new
            .map(|n| n.path.clone())
            .unwrap_or(xor.map(|x| x.path.clone()).unwrap());
        let stbuf = translate_and_stat(&path, fspath);
        match stbuf {
            Err(ref e) if e.kind() == ErrorKind::NotFound => Ok(bilog_dir {
                path,
                mode: 0,
                dir_exists: false,
                s: PhantomData,
            }),
            Err(e) => Err(e),
            Ok(stbuf) => Ok(bilog_dir {
                path,
                mode: stbuf.st_mode,
                dir_exists: true,
                s: PhantomData,
            }),
        }
    }
}
bilog_entry!(bilog_symlink {
    from: CString,
    to: CString,
    to_exists: bool
});

impl Bilog for bilog_symlink<Xor> {
    type N = bilog_symlink<New>;
    type O = bilog_symlink<Old>;
    type X = bilog_symlink<Xor>;
    fn new(call: &VFSCall) -> Self::N {
        match call {
            VFSCall::symlink(s) => bilog_symlink {
                from: s.from.clone().into_owned(),
                to: s.to.clone().into_owned(),
                to_exists: false,
                s: PhantomData,
            },
            VFSCall::unlink(u) => bilog_symlink {
                from: CString::new("").unwrap(),
                to: u.path.clone().into_owned(),
                to_exists: true,
                s: PhantomData,
            },
            _ => panic!("Cannot generate from {:?}", call),
        }
    }
    fn xor(o: &Self::O, _: &Self::N) -> Self::X {
        bilog_symlink {
            from: o.from.clone(),
            to: o.to.clone(),
            to_exists: true,
            s: PhantomData,
        }
    }
    fn apply<'a>(x: &'a Self::X, o: &Self::O) -> VFSCall<'a> {
        if o.to_exists {
            VFSCall::unlink(unlink {
                path: Cow::Borrowed(&x.to),
            })
        } else {
            VFSCall::symlink(symlink {
                from: Cow::Borrowed(&x.from),
                to: Cow::Borrowed(&x.to),
            })
        }
    }
    fn old(new: Option<&Self::N>, xor: Option<&Self::X>, fspath: &str) -> Result<Self::O, Error> {
        let from = new
            .map(|n| n.from.clone())
            .unwrap_or(xor.map(|x| x.from.clone()).unwrap());
        let to = new
            .map(|n| n.to.clone())
            .unwrap_or(xor.map(|x| x.to.clone()).unwrap());
        let stbuf = translate_and_stat(&to, fspath);
        if let Err(e) = stbuf {
            if e.kind() == ErrorKind::NotFound {
                return Ok(bilog_symlink {
                    from: CString::new("").unwrap(),
                    to: to,
                    to_exists: false,
                    s: PhantomData,
                });
            }
            return Err(e);
        }
        Ok(bilog_symlink {
            from,
            to,
            to_exists: true,
            s: PhantomData,
        })
    }
}
bilog_entry!(bilog_link {
    from: CString,
    to: CString,
    to_exists: bool
});
impl Bilog for bilog_link<Xor> {
    type N = bilog_link<New>;
    type O = bilog_link<Old>;
    type X = bilog_link<Xor>;
    fn new(call: &VFSCall) -> Self::N {
        match call {
            VFSCall::symlink(s) => bilog_link {
                from: s.from.clone().into_owned(),
                to: s.to.clone().into_owned(),
                to_exists: false,
                s: PhantomData,
            },
            VFSCall::unlink(u) => bilog_link {
                from: CString::new("").unwrap(),
                to: u.path.clone().into_owned(),
                to_exists: true,
                s: PhantomData,
            },
            _ => panic!("Cannot generate from {:?}", call),
        }
    }
    fn xor(o: &Self::O, _: &Self::N) -> Self::X {
        bilog_link {
            from: o.from.clone(),
            to: o.to.clone(),
            to_exists: true,
            s: PhantomData,
        }
    }
    fn apply<'a>(x: &'a Self::X, o: &Self::O) -> VFSCall<'a> {
        if o.to_exists {
            VFSCall::unlink(unlink {
                path: Cow::Borrowed(&x.to),
            })
        } else {
            VFSCall::link(link {
                from: Cow::Borrowed(&x.from),
                to: Cow::Borrowed(&x.to),
            })
        }
    }
    fn old(new: Option<&Self::N>, xor: Option<&Self::X>, fspath: &str) -> Result<Self::O, Error> {
        let to = new
            .map(|n| n.to.clone())
            .unwrap_or(xor.map(|x| x.to.clone()).unwrap());
        let stbuf = translate_and_stat(&to, fspath);

        match stbuf {
            Err(ref e) if e.kind() == ErrorKind::NotFound => Ok(bilog_link {
                from: CString::new("").unwrap(),
                to: to,
                to_exists: false,
                s: PhantomData,
            }),
            Err(e) => Err(e),
            Ok(stbuf) => {
                let from = find_hardlink(stbuf.st_ino, fspath)?;
                if from.is_none() {
                    return Err(Error::new(
                        ErrorKind::Other,
                        "File is hardlinked outside fsyncer path",
                    ));
                }

                Ok(bilog_link {
                    from: CString::new(from.unwrap()).unwrap(),
                    to,
                    to_exists: true,
                    s: PhantomData,
                })
            }
        }
    }
}
path_bilog!(bilog_node {
    mode: uint32_t,
    rdev: uint64_t,
    exists: bool
});
impl Bilog for bilog_node<Xor> {
    type N = bilog_node<New>;
    type O = bilog_node<Old>;
    type X = bilog_node<Xor>;
    fn new(call: &VFSCall) -> Self::N {
        match call {
            VFSCall::mknod(s) => bilog_node {
                path: s.path.clone().into_owned(),
                mode: s.mode,
                rdev: s.rdev,
                exists: false,
                s: PhantomData,
            },
            VFSCall::unlink(u) => bilog_node {
                path: u.path.clone().into_owned(),
                mode: 0,
                rdev: 0,
                exists: true,
                s: PhantomData,
            },
            _ => panic!("Cannot generate from {:?}", call),
        }
    }
    fn xor(o: &Self::O, n: &Self::N) -> Self::X {
        bilog_node {
            path: o.path.clone(),
            mode: if o.exists { n.mode } else { o.mode },
            rdev: if o.exists { n.rdev } else { o.rdev },
            exists: true,
            s: PhantomData,
        }
    }
    fn apply<'a>(x: &'a Self::X, o: &Self::O) -> VFSCall<'a> {
        if o.exists {
            VFSCall::unlink(unlink {
                path: Cow::Borrowed(&x.path),
            })
        } else {
            VFSCall::mknod(mknod {
                path: Cow::Borrowed(&x.path),
                mode: x.mode,
                rdev: x.rdev,
            })
        }
    }
    fn old(new: Option<&Self::N>, xor: Option<&Self::X>, fspath: &str) -> Result<Self::O, Error> {
        let path = new
            .map(|n| n.path.clone())
            .unwrap_or(xor.map(|x| x.path.clone()).unwrap());
        let stbuf = translate_and_stat(&path, fspath);
        if let Err(e) = stbuf {
            if e.kind() == ErrorKind::NotFound {
                return Ok(bilog_node {
                    path: path,
                    mode: 0,
                    rdev: 0,
                    exists: false,
                    s: PhantomData,
                });
            }
            return Err(e);
        }
        let stbuf = stbuf?;
        Ok(bilog_node {
            path: path,
            mode: stbuf.st_mode,
            rdev: stbuf.st_rdev,
            exists: true,
            s: PhantomData,
        })
    }
}
path_bilog!(bilog_file {
    mode: uint32_t,
    exists: bool
});
impl Bilog for bilog_file<Xor> {
    type N = bilog_file<New>;
    type O = bilog_file<Old>;
    type X = bilog_file<Xor>;
    fn new(call: &VFSCall) -> Self::N {
        match call {
            VFSCall::mknod(s) => bilog_file {
                path: s.path.clone().into_owned(),
                mode: s.mode,
                exists: false,
                s: PhantomData,
            },
            VFSCall::unlink(u) => bilog_file {
                path: u.path.clone().into_owned(),
                mode: 0,
                exists: true,
                s: PhantomData,
            },
            _ => panic!("Cannot generate from {:?}", call),
        }
    }
    fn xor(o: &Self::O, n: &Self::N) -> Self::X {
        bilog_file {
            path: o.path.clone(),
            mode: if o.exists { n.mode } else { o.mode },
            exists: true,
            s: PhantomData,
        }
    }
    fn apply<'a>(x: &'a Self::X, o: &Self::O) -> VFSCall<'a> {
        if o.exists {
            VFSCall::unlink(unlink {
                path: Cow::Borrowed(&x.path),
            })
        } else {
            VFSCall::create(create {
                path: Cow::Borrowed(&x.path),
                mode: x.mode,
                flags: 0,
            })
        }
    }
    fn old(new: Option<&Self::N>, xor: Option<&Self::X>, fspath: &str) -> Result<Self::O, Error> {
        let path = new
            .map(|n| n.path.clone())
            .unwrap_or(xor.map(|x| x.path.clone()).unwrap());
        let stbuf = translate_and_stat(&path, fspath);
        if let Err(e) = stbuf {
            if e.kind() == ErrorKind::NotFound {
                return Ok(bilog_file {
                    path: path,
                    mode: 0,
                    exists: false,
                    s: PhantomData,
                });
            }
            return Err(e);
        }
        let stbuf = stbuf?;
        Ok(bilog_file {
            path: path,
            mode: stbuf.st_mode,
            exists: true,
            s: PhantomData,
        })
    }
}
path_bilog!( bilog_truncate {
    size: int64_t,
    buf: Vec<u8>
});
impl Bilog for bilog_truncate<Xor> {
    type N = bilog_truncate<New>;
    type O = bilog_truncate<Old>;
    type X = bilog_truncate<Xor>;
    fn new(call: &VFSCall) -> Self::N {
        if let VFSCall::truncate(c) = call {
            bilog_truncate {
                path: c.path.clone().into_owned(),
                size: c.size,
                buf: Vec::new(),
                s: PhantomData,
            }
        } else {
            panic!("Cannot generate from {:?}", call)
        }
    }
    fn xor(o: &Self::O, n: &Self::N) -> Self::X {
        bilog_truncate {
            path: n.path.clone(),
            size: o.size ^ n.size,
            buf: o.buf.clone(),
            s: PhantomData,
        }
    }
    fn apply<'a>(x: &'a Self::X, o: &Self::O) -> VFSCall<'a> {
        let nsize = x.size ^ o.size;
        if nsize > o.size && x.buf.len() != 0 {
            VFSCall::write(write {
                path: Cow::Borrowed(&x.path),
                offset: o.size,
                buf: Cow::Borrowed(&x.buf),
            })
        } else {
            VFSCall::truncate(truncate {
                path: Cow::Borrowed(&x.path),
                size: x.size ^ o.size,
            })
        }
    }
    fn old(new: Option<&Self::N>, xor: Option<&Self::X>, fspath: &str) -> Result<Self::O, Error> {
        let path = new
            .map(|n| n.path.clone())
            .unwrap_or(xor.map(|x| x.path.clone()).unwrap());
        let stbuf = translate_and_stat(&path, fspath)?;
        let osize = stbuf.st_size;
        let nsize = new
            .map(|n| n.size)
            .unwrap_or(xor.map(|x| x.size ^ osize).unwrap());

        let mut buf = Vec::new();

        if osize < nsize {
            let real_path = translate_path(&path, &fspath);
            let f = File::open(OsStr::from_bytes(&real_path.to_bytes()))?;

            buf.reserve((nsize - osize) as usize);
            unsafe { buf.set_len((nsize - osize) as usize) };
            f.read_exact_at(&mut buf[..], osize as u64)?;
        }

        Ok(bilog_truncate {
            path: path.clone(),
            size: stbuf.st_size,
            buf: buf,
            s: PhantomData,
        })
    }
}
path_bilog!( bilog_write {
    offset: int64_t,
    buf: Vec<u8>
});
impl Bilog for bilog_write<Xor> {
    type N = bilog_write<New>;
    type O = bilog_write<Old>;
    type X = bilog_write<Xor>;
    fn new(call: &VFSCall) -> Self::N {
        if let VFSCall::write(c) = call {
            bilog_write {
                path: c.path.clone().into_owned(),
                offset: c.offset,
                buf: c.buf.clone().into_owned(),
                s: PhantomData,
            }
        } else {
            panic!("Cannot generate from {:?}", call)
        }
    }
    fn xor(o: &Self::O, n: &Self::N) -> Self::X {
        let mut buf = n.buf.clone();
        xor_buf(&mut buf, &o.buf);
        bilog_write {
            path: n.path.clone(),
            offset: o.offset,
            buf: buf,
            s: PhantomData,
        }
    }
    fn apply<'a>(x: &'a Self::X, o: &Self::O) -> VFSCall<'a> {
        let buf = if o.buf.len() == 0 {
            //Appending write
            Cow::Borrowed(&x.buf[..])
        } else {
            let mut xbuf = x.buf.clone();
            xor_buf(&mut xbuf, &o.buf);
            Cow::Owned(xbuf)
        };
        VFSCall::write(write {
            path: Cow::Borrowed(&x.path),
            offset: x.offset,
            buf: buf,
        })
    }
    fn old(new: Option<&Self::N>, xor: Option<&Self::X>, fspath: &str) -> Result<Self::O, Error> {
        let path = new
            .map(|n| n.path.clone())
            .unwrap_or(xor.map(|x| x.path.clone()).unwrap());
        let offset = new
            .map(|n| n.offset)
            .unwrap_or(xor.map(|x| x.offset).unwrap());
        let write_len = new
            .map(|n| n.buf.len())
            .unwrap_or(xor.map(|x| x.buf.len()).unwrap());
        let real_path = translate_path(&path, fspath);

        let f = File::open(OsStr::from_bytes(&real_path.to_bytes()))?;
        let osize = f.metadata()?.len();

        let mut buf = Vec::new();

        if osize > offset as u64 {
            // Not an appending write
            let overlap = min(osize - offset as u64, write_len as u64) as usize;
            buf.reserve(overlap);
            unsafe { buf.set_len(overlap) };
            f.read_exact_at(&mut buf[..], offset as u64)?;
        }

        Ok(bilog_write {
            path: path.clone(),
            offset: offset,
            buf: buf,
            s: PhantomData,
        })
    }
}
path_bilog!( bilog_xattr {
    name: CString,
    value: Option<Vec<u8>>,
    remove: bool
});
impl Bilog for bilog_xattr<Xor> {
    type N = bilog_xattr<New>;
    type O = bilog_xattr<Old>;
    type X = bilog_xattr<Xor>;
    fn new(call: &VFSCall) -> Self::N {
        match call {
            VFSCall::setxattr(s) => bilog_xattr {
                path: s.path.clone().into_owned(),
                name: s.name.clone().into_owned(),
                value: Some(s.value.clone().into_owned()),
                remove: false,
                s: PhantomData,
            },
            VFSCall::removexattr(u) => bilog_xattr {
                path: u.path.clone().into_owned(),
                name: u.name.clone().into_owned(),
                value: None,
                remove: true,
                s: PhantomData,
            },
            _ => panic!("Cannot generate from {:?}", call),
        }
    }
    fn xor(o: &Self::O, n: &Self::N) -> Self::X {
        let mut remove = false;
        let value = if n.value.is_none() {
            remove = true;
            o.value
                .as_ref()
                .expect("If new state removes the value, oldstate must have it")
                .clone()
        } else {
            if o.value.is_none() {
                // Newstate sets the value
                remove = true;
                n.value
                    .as_ref()
                    .expect("If old state doesn't have the value, newstate must set it")
                    .clone()
            } else {
                let ovalue = o.value.as_ref().unwrap();
                let nvalue = n.value.as_ref().unwrap();
                if ovalue.len() > nvalue.len() {
                    let mut buf = ovalue.clone();
                    xor_buf(&mut buf, nvalue);
                    buf
                } else {
                    let mut buf = nvalue.clone();
                    xor_buf(&mut buf, ovalue);
                    buf
                }
            }
        };
        bilog_xattr {
            path: n.path.clone(),
            name: n.name.clone(),
            value: Some(value),
            remove,
            s: PhantomData,
        }
    }
    fn apply<'a>(x: &'a Self::X, o: &Self::O) -> VFSCall<'a> {
        if o.value.is_some() {
            if x.remove {
                VFSCall::removexattr(removexattr {
                    path: Cow::Borrowed(&x.path),
                    name: Cow::Borrowed(&x.name),
                })
            } else {
                let mut value = x.value.as_ref().unwrap().clone();
                let ovalue = o.value.as_ref().unwrap();
                for v in 0..value.len() {
                    value[v] ^= ovalue[v];
                }
                VFSCall::setxattr(setxattr {
                    path: Cow::Borrowed(&x.path),
                    name: Cow::Borrowed(&x.name),
                    value: Cow::Owned(value),
                    flags: 0,
                })
            }
        } else {
            VFSCall::setxattr(setxattr {
                path: Cow::Borrowed(&x.path),
                name: Cow::Borrowed(&x.name),
                value: Cow::Borrowed(
                    x.value
                        .as_ref()
                        .expect("Old value is not set, xor state must have it"),
                ),
                flags: 0,
            })
        }
    }
    fn old(new: Option<&Self::N>, xor: Option<&Self::X>, fspath: &str) -> Result<Self::O, Error> {
        let path = new
            .map(|n| n.path.clone())
            .unwrap_or(xor.map(|x| x.path.clone()).unwrap());
        let name = new
            .map(|n| n.name.clone())
            .unwrap_or(xor.map(|x| x.name.clone()).unwrap());
        let real_path = translate_path(&path, &fspath);
        let mut val_buf: [u8; 4096] = [0; 4096];
        let len = unsafe {
            lgetxattr(
                real_path.as_ptr(),
                name.as_ptr(),
                val_buf.as_mut_ptr() as *mut _,
                4096, // HACK, I should query the size first
            )
        };
        if len == -1 {
            let err = errno();
            let interr: i32 = err.into();
            if interr == ENOATTR {
                return Ok(bilog_xattr {
                    path: path,
                    name: name,
                    value: None,
                    remove: true,
                    s: PhantomData,
                });
            }
            return Err(Error::from(err));
        }
        // Value exists
        Ok(bilog_xattr {
            path: path,
            name: name,
            value: Some(Vec::from(&val_buf[..len as usize])),
            remove: false,
            s: PhantomData,
        })
    }
}

fn xor_buf(new: &mut Vec<u8>, old: &Vec<u8>) {
    assert!(new.len() >= old.len());
    for i in 0..old.len() {
        new[i] ^= old[i];
    }
}
