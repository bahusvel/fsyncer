use std::convert::TryFrom;
use std::io;
use std::path::Path;

use common::VFSCall;
use error::Error;
use journal::JournalEntry;

impl TryFrom<(&VFSCall<'_>, &Path)> for VFSCall<'_> {
    type Error = Error<io::Error>;
    fn try_from(_: (&VFSCall<'_>, &Path)) -> Result<Self, Self::Error> {
        panic!("You don't need this, just use the value directly")
    }
}

// VFSCall is the entry for forward journal, hence this.
impl JournalEntry<'_> for VFSCall<'_> {
    fn apply(&self, _: &Path) -> Result<VFSCall, Error<io::Error>> {
        Ok(self.clone())
    }
    fn affected_paths(&self) -> Vec<&Path> {
        match self {
            VFSCall::chmod { path, .. } => vec![path],
            VFSCall::security { path, .. } => vec![path],
            VFSCall::utimens { path, .. } => vec![path],
            VFSCall::rename { from, to, .. } => vec![from, to],
            VFSCall::mkdir { path, .. } => vec![path],
            VFSCall::rmdir { path, .. } => vec![path],
            VFSCall::symlink { to, .. } => vec![to],
            VFSCall::link { to, .. } => vec![to],
            VFSCall::mknod { path, .. } => vec![path],
            VFSCall::unlink { path, .. } => vec![path],
            VFSCall::create { path, .. } => vec![path],
            VFSCall::truncate { path, .. } => vec![path],
            VFSCall::write { path, .. } => vec![path],
            VFSCall::diff_write { path, .. } => vec![path],
            VFSCall::fallocate { path, .. } => vec![path],
            VFSCall::setxattr { path, .. } => vec![path],
            VFSCall::removexattr { path, .. } => vec![path],
            VFSCall::fsync { path, .. } => vec![path],
            VFSCall::truncating_write { path, .. } => vec![path],
        }
    }
}
