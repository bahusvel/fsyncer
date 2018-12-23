use common::*;
use journal::*;
use std::io::Error;

pub trait BilogItem: LogItem {
    fn gen_bilog(oldstate: Self, newstate: Self) -> Self;
    fn describe_bilog(&self, detail: bool) -> String;
    fn apply_bilog(&self, fspath: &str, current_state: &Self) -> i32;
}
//  idealistic type to express these things
//use std::marker::PhantomData;

// trait BilogState {}
// struct Old {}
// impl BilogState for Old {}
// struct New {}
// impl BilogState for New {}
// struct Xor {}
// impl BilogState for Xor {}

// #[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
// struct bilog_chmod<S: BilogState> {
//     path: CString,
//     mode: mode_t,
//     s: PhantomData<S>,
// }

// trait NewState {}
// impl NewState for bilog_chmod<New> {}

// trait ProperBilog: Sized {
//     type N: NewState;
//     type O;
//     type X;
//     fn from_vfscall(call: VFSCall) -> Self::N;
//     fn gen_bilog(o: Self::O, n: Self::N) -> Self::X;
//     fn appy_bilog(x: Self::X, o: Self::O) -> Self::N;
// }
// /*
// impl ProperBilog for bilog_chmod {
//     type N = bilog_chmod<New>;
//     type O = bilog_chmod<Old>;
//     type X = bilog_chmod<Xor>;
// }*/
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
}
