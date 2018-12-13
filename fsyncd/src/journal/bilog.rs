use common::*;
use journal::*;

pub trait BilogItem: LogItem {
    fn gen_bilog(oldstate: Self, newstate: Self) -> Self;
}

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
}

impl BilogItem for log_chown {
    fn gen_bilog(oldstate: Self, newstate: Self) -> Self {
        log_chown(chown {
            path: newstate.0.path,
            uid: newstate.0.uid ^ oldstate.0.uid,
            gid: newstate.0.gid ^ newstate.0.gid,
        })
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
}

impl BilogItem for log_rename {
    fn gen_bilog(_oldstate: Self, newstate: Self) -> Self {
        newstate
    }
}

impl BilogItem for log_dir {
    fn gen_bilog(oldstate: Self, _newstate: Self) -> Self {
        oldstate
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
}
