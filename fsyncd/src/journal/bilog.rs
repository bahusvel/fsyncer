use common::*;
use journal::*;
use serde::{Deserialize, Serialize};
use std::io::Error;

pub trait BilogItem: LogItem {
    fn gen_bilog(oldstate: Self, newstate: Self) -> Self;
    fn describe_bilog(&self, detail: bool) -> String;
    fn apply_bilog(&self, fspath: &str, current_state: &Self) -> i32;
}
//  idealistic type to express these things
use std::marker::PhantomData;

trait BilogState {}
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

trait JournalEntry<'a>: Serialize + Deserialize<'a> + Clone {
    fn from_vfscall(call: VFSCall) -> Result<Self, Error>;
    fn display(&self, detail: bool) -> String;
    fn apply(&self) -> Result<VFSCall, Error>;
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum BilogEntry {
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
    /*
    fn from_vfscall(call: &VFSCall) -> Result<Self, Error> {
        Ok(match call {
            VFSCall::mknod(m) => BilogEntry::node(bilog_node {
                path: m.path.clone().into_owned(),
                mode: m.mode,
                rdev: m.rdev,
            }),
            VFSCall::mkdir(m) => BilogEntry::dir(bilog_dir {
                path: m.path.clone().into_owned(),
                mode: m.mode,
                dir_exists: false,
            }),
            VFSCall::unlink(u) => BilogEntry::log_file(log_file::unlink(unlink {
                path: Cow::Owned(u.path.clone().into_owned()),
            })),
            VFSCall::rmdir(r) => BilogEntry::dir(bilog_dir {
                path: r.path.clone().into_owned(),
                mode: 0,
                dir_exists: true,
            }),
            VFSCall::symlink(s) => BilogEntry::symlink(bilog_symlink {
                from: s.from.clone().into_owned(),
                to: s.to.clone().into_owned(),
            }),
            VFSCall::rename(r) => BilogEntry::rename(bilog_rename {
                from: r.from.clone().into_owned(),
                to: r.to.clone().into_owned(),
                from_exists: true,
            }),
            VFSCall::link(l) => BilogEntry::link(bilog_link {
                from: l.from.clone().into_owned(),
                to: l.to.clone().into_owned(),
            }),
            VFSCall::chmod(c) => BilogEntry::log_chmod(log_chmod(chmod {
                path: Cow::Owned(c.path.clone().into_owned()),
                mode: c.mode,
            })),
            VFSCall::chown(c) => BilogEntry::log_chown(log_chown(chown {
                path: Cow::Owned(c.path.clone().into_owned()),
                uid: c.uid,
                gid: c.gid,
            })),
            VFSCall::truncate(t) => BilogEntry::log_write(log_write {
                path: t.path.clone().into_owned(),
                offset: 0,
                size: t.size,
                buf: Vec::new(),
            }),
            VFSCall::write(w) => BilogEntry::log_write(log_write {
                path: w.path.clone().into_owned(),
                offset: w.offset,
                size: w.offset + w.buf.len() as i64,
                buf: w.buf.clone().into_owned(),
            }),
            VFSCall::setxattr(s) => BilogEntry::log_xattr(log_xattr {
                path: s.path.clone().into_owned(),
                name: s.name.clone().into_owned(),
                value: Some(s.value.clone().into_owned()),
            }),
            VFSCall::removexattr(r) => BilogEntry::log_xattr(log_xattr {
                path: r.path.clone().into_owned(),
                name: r.name.clone().into_owned(),
                value: None,
            }),
            VFSCall::create(c) => BilogEntry::log_file(log_file::file(create {
                path: Cow::Owned(c.path.clone().into_owned()),
                mode: c.mode,
                flags: c.flags,
            })),
            VFSCall::utimens(u) => BilogEntry::log_utimens(log_utimens(utimens {
                path: Cow::Owned(u.path.clone().into_owned()),
                timespec: u.timespec.clone(),
            })),
            VFSCall::fallocate(_fallocate) => panic!("Not implemented"),
            VFSCall::fsync(_fsync) => panic!("Not an IO call"),
        })
    }
    */
    fn display(&self, detail: bool) -> String {}
    fn apply(&self) -> Result<VFSCall, Error> {}
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
            path: n.path,
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
        let path = new.map(|n| n.path).unwrap_or(xor.map(|x| x.path).unwrap());
        let stbuf = translate_and_stat(&path, fspath)?;
        Ok(bilog_chmod {
            path: path.clone(),
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
            path: n.path,
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
        let path = new.map(|n| n.path).unwrap_or(xor.map(|x| x.path).unwrap());
        let stbuf = translate_and_stat(&path, fspath)?;
        Ok(bilog_chown {
            path: path.clone(),
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
                timespec: c.timespec,
                s: PhantomData,
            }
        } else {
            panic!("Cannot generate from {:?}", call)
        }
    }
    fn xor(o: &Self::O, n: &Self::N) -> Self::X {
        bilog_utimens {
            path: n.path,
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
        let path = new.map(|n| n.path).unwrap_or(xor.map(|x| x.path).unwrap());
        let stbuf = translate_and_stat(&path, fspath)?;
        Ok(bilog_utimens {
            path: path.clone(),
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
    fn xor(o: &Self::O, n: &Self::N) -> Self::X {
        bilog_rename {
            from: o.from,
            to: o.to,
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
    fn old(new: Option<&Self::N>, xor: Option<&Self::X>, fspath: &str) -> Result<Self::O, Error> {
        let from = new.map(|n| n.from).unwrap_or(xor.map(|x| x.from).unwrap());
        let to = new.map(|n| n.to).unwrap_or(xor.map(|x| x.to).unwrap());
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
            path: o.path,
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
        let path = new.map(|n| n.path).unwrap_or(xor.map(|x| x.path).unwrap());
        let stbuf = translate_and_stat(&path, fspath);
        if let Err(e) = stbuf {
            if e.kind() == ErrorKind::NotFound {
                return Ok(bilog_dir {
                    path,
                    mode: 0,
                    dir_exists: false,
                    s: PhantomData,
                });
            }
        }
        let stbuf = stbuf?;
        Ok(bilog_dir {
            path,
            mode: stbuf.st_mode,
            dir_exists: true,
            s: PhantomData,
        })
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
    fn xor(o: &Self::O, n: &Self::N) -> Self::X {
        bilog_symlink {
            from: o.from,
            to: o.to,
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
        let from = new.map(|n| n.from).unwrap_or(xor.map(|x| x.from).unwrap());
        let to = new.map(|n| n.to).unwrap_or(xor.map(|x| x.to).unwrap());
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
        }
        let stbuf = stbuf?;
        Ok(bilog_symlink {
            from,
            to,
            from_exists: true,
            s: PhantomData,
        })
    }
}
bilog_entry!(bilog_link {
    from: CString,
    to: CString,
    to_exists: bool
});
path_bilog!(bilog_node {
    mode: uint32_t,
    rdev: uint64_t,
    exists: bool
});
path_bilog!(bilog_file {
    mode: uint32_t,
    exists: bool
});
path_bilog!( bilog_truncate {
    size: int64_t,
    buf: Vec<u8>
});
path_bilog!( bilog_write {
    offset: int64_t,
    buf: Vec<u8>
});
path_bilog!( bilog_xattr {
    name: CString,
    value: Option<Vec<u8>>
});

/*
impl ProperBilog for bilog_chmod {
    type N = bilog_chmod<New>;
    type O = bilog_chmod<Old>;
    type X = bilog_chmod<Xor>;
}*/
impl BilogItem for JournalCall {
    fn gen_bilog(oldstate: Self, newstate: Self) -> Self {
        match (oldstate, newstate) {
            (JournalCall::log_chmod(o), JournalCall::log_chmod(n)) => {
                JournalCall::log_chmod(BilogItem::gen_bilog(o, n))
            }
            (JournalCall::log_chown(o), JournalCall::log_chown(n)) => {
                JournalCall::log_chown(BilogItem::gen_bilog(o, n))
            }
            (JournalCall::log_utimens(o), JournalCall::log_utimens(n)) => {
                JournalCall::log_utimens(BilogItem::gen_bilog(o, n))
            }
            (JournalCall::log_rename(o), JournalCall::log_rename(n)) => {
                JournalCall::log_rename(BilogItem::gen_bilog(o, n))
            }
            (JournalCall::log_dir(o), JournalCall::log_dir(n)) => {
                JournalCall::log_dir(BilogItem::gen_bilog(o, n))
            }
            (JournalCall::log_file(o), JournalCall::log_file(n)) => {
                JournalCall::log_file(BilogItem::gen_bilog(o, n))
            }
            (JournalCall::log_xattr(o), JournalCall::log_xattr(n)) => {
                JournalCall::log_xattr(BilogItem::gen_bilog(o, n))
            }
            (JournalCall::log_write(o), JournalCall::log_write(n)) => {
                JournalCall::log_write(BilogItem::gen_bilog(o, n))
            }
            (o, n) => panic!(
                "Impossible combination of newstate {:?} and oldstate {:?}",
                n, o
            ),
        }
    }
    fn describe_bilog(&self, detail: bool) -> String {
        match self {
            JournalCall::log_chmod(o) => o.describe_bilog(detail),
            JournalCall::log_chown(o) => o.describe_bilog(detail),
            JournalCall::log_utimens(o) => o.describe_bilog(detail),
            JournalCall::log_rename(o) => o.describe_bilog(detail),
            JournalCall::log_dir(o) => o.describe_bilog(detail),
            JournalCall::log_file(o) => o.describe_bilog(detail),
            JournalCall::log_xattr(o) => o.describe_bilog(detail),
            JournalCall::log_write(o) => o.describe_bilog(detail),
        }
    }
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

impl BilogItem for log_chmod {
    fn gen_bilog(oldstate: Self, newstate: Self) -> Self {
        log_chmod(chmod {
            path: newstate.0.path,
            mode: newstate.0.mode ^ oldstate.0.mode,
        })
    }
    fn describe_bilog(&self, detail: bool) -> String {
        if detail {
            format!("{:?}", self)
        } else {
            format!("{:?} changed permissions", self.0.path)
        }
    }
    fn apply_bilog(&self, fspath: &str, current_state: &Self) -> i32 {
        let path = translate_path(&self.0.path, fspath);
        let mode = self.0.mode ^ current_state.0.mode;
        xmp_chmod(path.as_ptr(), mode, -1)
    }
}

impl BilogItem for log_chown {
    fn gen_bilog(oldstate: Self, newstate: Self) -> Self {
        log_chown(chown {
            path: newstate.0.path,
            uid: newstate.0.uid ^ oldstate.0.uid,
            gid: newstate.0.gid ^ newstate.0.gid,
        })
    }
    fn describe_bilog(&self, detail: bool) -> String {
        if detail {
            format!("{:?}", self)
        } else {
            format!("{:?} changed onwership", self.0.path)
        }
    }
    fn apply_bilog(&self, fspath: &str, current_state: &Self) -> i32 {
        let path = translate_path(&self.0.path, fspath);
        let uid = self.0.uid ^ current_state.0.uid;
        let gid = self.0.gid ^ current_state.0.gid;
        xmp_chown(path.as_ptr(), uid, gid, -1)
    }
}

impl BilogItem for log_utimens {
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
    fn describe_bilog(&self, detail: bool) -> String {
        if detail {
            format!("{:?}", self)
        } else {
            format!("{:?} changed mtime/ctime", self.0.path)
        }
    }
    fn apply_bilog(&self, fspath: &str, current_state: &Self) -> i32 {
        let path = translate_path(&self.0.path, fspath);
        let ts = [
            timespec {
                tv_sec: current_state.0.timespec[0].tv_sec ^ self.0.timespec[0].tv_sec,
                tv_nsec: current_state.0.timespec[0].tv_nsec ^ self.0.timespec[0].tv_nsec,
            },
            timespec {
                tv_sec: current_state.0.timespec[1].tv_sec ^ self.0.timespec[1].tv_sec,
                tv_nsec: current_state.0.timespec[1].tv_nsec ^ self.0.timespec[1].tv_nsec,
            },
        ];
        xmp_utimens(path.as_ptr(), &ts as *const timespec, -1)
    }
}

impl BilogItem for log_rename {
    fn gen_bilog(_oldstate: Self, newstate: Self) -> Self {
        newstate
    }
    fn describe_bilog(&self, detail: bool) -> String {
        if detail {
            format!("{:?}", self)
        } else {
            format!("{:?} renamed to {:?}", self.from, self.to)
        }
    }
    fn apply_bilog(&self, fspath: &str, current_state: &Self) -> i32 {
        let from = translate_path(&self.from, fspath);
        let to = translate_path(&self.to, fspath);
        if current_state.from_exists {
            xmp_rename(from.as_ptr(), to.as_ptr(), 0)
        } else {
            xmp_rename(to.as_ptr(), from.as_ptr(), 0)
        }
    }
}

impl BilogItem for log_dir {
    fn gen_bilog(oldstate: Self, _newstate: Self) -> Self {
        oldstate
    }
    fn describe_bilog(&self, detail: bool) -> String {
        if detail {
            format!("{:?}", self)
        } else {
            format!("{:?} created or removed directory", self.path)
        }
    }
    fn apply_bilog(&self, fspath: &str, current_state: &Self) -> i32 {
        let path = translate_path(&self.path, fspath);
        if current_state.dir_exists {
            xmp_rmdir(path.as_ptr())
        } else {
            xmp_mkdir(path.as_ptr(), self.mode)
        }
    }
}

impl BilogItem for log_file {
    fn gen_bilog(oldstate: Self, newstate: Self) -> Self {
        if is_variant!(newstate, log_file::unlink) {
            oldstate
        } else {
            newstate
        }
    }
    fn describe_bilog(&self, detail: bool) -> String {
        if detail {
            format!("{:?}", self)
        } else {
            let file_type = match self {
                log_file::symlink(_) => "a symlink",
                log_file::link(_) => "a hardlink",
                log_file::node(_) => "a node",
                log_file::file(_) => "a plain file",
                log_file::unlink(_) => "an unkown file type",
            };
            format!("{:?} created or removed as {}", self.file_path(), file_type)
        }
    }
    fn apply_bilog(&self, fspath: &str, current_state: &Self) -> i32 {
        if is_variant!(current_state, log_file::unlink) {
            // Need to create the file
            match self {
                log_file::symlink(symlink { from, to }) => {
                    let to = translate_path(to, fspath);
                    xmp_symlink(from.as_ptr(), to.as_ptr())
                }
                log_file::link(link { from, to }) => {
                    let from = translate_path(from, fspath);
                    let to = translate_path(to, fspath);
                    xmp_link(from.as_ptr(), to.as_ptr())
                }
                log_file::node(mknod { path, mode, rdev }) => {
                    let path = translate_path(path, fspath);
                    xmp_mknod(path.as_ptr(), *mode, *rdev)
                }
                log_file::file(create { path, mode, flags }) => {
                    let path = translate_path(path, fspath);
                    let mut fd = -1;
                    let res = xmp_create(path.as_ptr(), *mode, &mut fd as *mut c_int, 0);
                    if fd != -1 {
                        close(fd);
                    }
                    res
                }
            }
        } else {
            // Need to remove the file
            let path = translate_path(self.file_path(), fspath);
            xmp_unlink(path.as_ptr())
        }
    }
}

impl BilogItem for log_xattr {
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
    fn describe_bilog(&self, detail: bool) -> String {
        if detail {
            format!("{:?}", self)
        } else {
            format!(
                "{:?} set, changed or removed extended attribute {:?}",
                self.path, self.name,
            )
        }
    }
    fn apply_bilog(&self, fspath: &str, current_state: &Self) -> i32 {
        let path = translate_path(&self.path, fspath);
        if let Some(v) = current_state.value {
            if self.value.is_none() {
                xmp_removexattr(path.as_ptr(), self.name.as_ptr())
            } else {
                let buf = xor_largest_buf(self.value.unwrap(), current_state.value.unwrap());
                xmp_setxattr(
                    path.as_ptr(),
                    self.name.as_ptr(),
                    buf.as_ptr(),
                    buf.len(),
                    0,
                )
            }
        } else {
            xmp_setxattr(
                path.as_ptr(),
                self.name.as_ptr(),
                self.value.unwrap().as_ptr(),
                self.value.unwrap().len(),
                0,
            )
        }
    }
}

impl BilogItem for log_write {
    fn gen_bilog(oldstate: Self, mut newstate: Self) -> Self {
        xor_buf(&mut newstate.buf, &oldstate.buf);
        log_write {
            path: newstate.path,
            offset: newstate.offset,
            size: oldstate.size ^ newstate.size,
            buf: newstate.buf,
        }
    }
    fn describe_bilog(&self, detail: bool) -> String {
        if detail {
            format!("{:?}", self)
        } else {
            format!(
                "{:?} wrote or extended the file at offset {}",
                self.path, self.offset
            )
        }
    }
    fn apply_bilog(&self, fspath: &str, current_state: &Self) -> i32 {
        let path = translate_path(&self.path, fspath);
        let nsize = self.size ^ current_state.size;
        if (nsize > current_state.size && self.buf.len() == 0) || nsize < current_state.size {
            // The operation is an extension or trimming truncate
            xmp_truncate(path.as_ptr(), nsize, -1)
        } else {
            // The operation is a write than extends the file or a normal write
            // Need to compute the overlapping componenet of the buffer
            let mut nbuf = self.buf.clone();
            xor_buf(&mut nbuf, &current_state.buf);
            xmp_write(path.as_ptr(), nbuf.as_ptr(), nbuf.len(), self.offset, -1)
        }
    }
}
