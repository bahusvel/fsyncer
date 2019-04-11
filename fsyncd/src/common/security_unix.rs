use common::ToCString;
use libc::*;
use std::os::unix::fs::MetadataExt;

pub fn copy_security(src: &Path, dst: &Path) -> Result<(), Error<io::Error>> {
    let stat = trace!(src.metadata());
    let dst_path = dst.to_owned().into_cstring();
    if unsafe { chown(dst_path.as_ptr(), stat.uid(), stat.gid()) } == -1 {
        trace!(Err(io::Error::last_os_error()));
    }
    if unsafe { chmod(dst_path.as_ptr(), stat.mode()) } == -1 {
        trace!(Err(io::Error::last_os_error()));
    }
    Ok(())
}
