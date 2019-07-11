use common::VFSCall;
use error::Error;
use std::convert::TryFrom;
use std::io;
use std::path::Path;

bitflags! {
    #[derive(Serialize, Deserialize)]
    pub struct StatChange: u8 {
        const MODE    = 0b0001;
        const UID_GID = 0b0010;
        const TIME    = 0b0100;
        const SIZE    = 0b1000;
    }
}

#[derive(Serialize, Deserialize)]
enum MDataEntry<'a> {
    New(&'a Path),
    Unlink(&'a Path),
    Rename(&'a Path, &'a Path),
    Link(&'a Path, &'a Path),
    Stat {
        path: &'a Path,
        fields: StatChange,
    },
    Xattr {
        path: &'a Path,
        name: &'a str,
    },
    Write {
        path: &'a Path,
        offset: u64,
        len: u64,
    },
}

impl<'a> TryFrom<(&'a VFSCall<'a>, &'static Path)> for MDataEntry<'a> {
    type Error = Error<io::Error>;
    fn try_from(
        (call, _): (&'a VFSCall<'a>, &Path),
    ) -> Result<Self, Self::Error> {
        Ok(match call {
            VFSCall::mkdir { path, .. }
            | VFSCall::mknod { path, .. }
            | VFSCall::create { path, .. } => MDataEntry::New(path),
            VFSCall::unlink { path, .. } | VFSCall::rmdir { path, .. } => {
                MDataEntry::Unlink(path)
            }
            VFSCall::setxattr { path, name, .. }
            | VFSCall::removexattr { path, name } => MDataEntry::Xattr {
                path,
                name: name.to_str().unwrap(),
            },
            VFSCall::chmod { path, .. } => MDataEntry::Stat {
                path,
                fields: StatChange::MODE,
            },
            VFSCall::symlink { from, to, .. }
            | VFSCall::link { from, to, .. } => MDataEntry::Link(from, to),
            VFSCall::rename { from, to, .. } => MDataEntry::Rename(from, to),
            VFSCall::security { path, .. } => MDataEntry::Stat {
                path,
                fields: StatChange::UID_GID,
            },
            VFSCall::truncate { path, .. } => MDataEntry::Stat {
                path,
                fields: StatChange::SIZE,
            },
            VFSCall::utimens { path, .. } => MDataEntry::Stat {
                path,
                fields: StatChange::TIME,
            },
            VFSCall::write { path, offset, buf }
            | VFSCall::diff_write { path, offset, buf } => MDataEntry::Write {
                path,
                offset: *offset as u64,
                len: buf.len() as u64,
            },
            VFSCall::fallocate { path, .. } => MDataEntry::Stat {
                path,
                fields: StatChange::SIZE,
            },
            VFSCall::fsync { .. } => panic!("Not an IO call"),
            VFSCall::truncating_write { .. } => panic!("Not a fuse syscall"),
        })
    }
}
