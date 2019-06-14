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
            VFSCall::chmod(c) => vec![&c.path],
            VFSCall::security(c) => vec![&c.path],
            VFSCall::utimens(c) => vec![&c.path],
            VFSCall::rename(c) => vec![&c.from, &c.to],
            VFSCall::mkdir(c) => vec![&c.path],
            VFSCall::rmdir(c) => vec![&c.path],
            VFSCall::symlink(c) => vec![&c.to],
            VFSCall::link(c) => vec![&c.to],
            VFSCall::mknod(c) => vec![&c.path],
            VFSCall::unlink(c) => vec![&c.path],
            VFSCall::create(c) => vec![&c.path],
            VFSCall::truncate(c) => vec![&c.path],
            VFSCall::write(c) => vec![&c.path],
            VFSCall::diff_write(c) => vec![&c.path],
            VFSCall::fallocate(c) => vec![&c.path],
            VFSCall::setxattr(c) => vec![&c.path],
            VFSCall::removexattr(c) => vec![&c.path],
            VFSCall::fsync(c) => vec![&c.path],
            VFSCall::truncating_write { write: c, .. } => vec![&c.path],
        }
    }
}
