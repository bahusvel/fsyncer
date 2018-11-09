use libc::*;
use server::fuseops::fuse_operations;
use server::fuseops::{fuse_config, fuse_conn_info};
use server::read::*;
use server::write::*;
use std::mem::size_of;
use std::ptr;

#[link(name = "fsyncer", kind = "static")]
#[link(name = "fuse3")]
extern "C" {
    fn fuse_main_real(
        argc: i32,
        argv: *const *const c_char,
        op: *const fuse_operations,
        op_size: size_t,
        private_data: *const c_void,
    ) -> c_int;
}

pub unsafe extern "C" fn xmp_init(conn: *mut fuse_conn_info, cfg: *mut fuse_config) -> *mut c_void {
    (*cfg).use_ino = 1;
    // NOTE this makes path NULL to parameters where fi->fh exists. This is evil
    // for the current case of replication. But in future when this is properly
    // handled it can improve performance.
    // refer to
    // https://libfuse.github.io/doxygen/structfuse__config.html#adc93fd1ac03d7f016d6b0bfab77f3863
    (*cfg).nullpath_ok = 1;

    /* Pick up changes from lower filesystem right away. This is
	   also necessary for better hardlink support. When the kernel
	   calls the unlink() handler, it does not know the inode of
	   the to-be-removed entry and can therefore not invalidate
	   the cache of the associated inode - resulting in an
	   incorrect st_nlink value being reported for any remaining
	   hardlinks to this inode. */
    // cfg->entry_timeout = 0;
    // cfg->attr_timeout = 0;
    // cfg->negative_timeout = 0;
    (*cfg).auto_cache = 1;
    (*conn).max_write = 32 * 1024;

    return ptr::null_mut();
}

pub unsafe fn fuse_main(argc: c_int, argv: *const *const c_char) -> c_int {
    let mut ops = fuse_operations::default();
    // Write ops
    ops.mknod = Some(do_mknod);
    ops.mkdir = Some(do_mkdir);
    ops.symlink = Some(do_symlink);
    ops.unlink = Some(do_unlink);
    ops.rmdir = Some(do_rmdir);
    ops.rename = Some(do_rename);
    ops.link = Some(do_link);
    ops.chmod = Some(do_chmod);
    ops.chown = Some(do_chown);
    ops.truncate = Some(do_truncate);
    ops.create = Some(do_create);
    ops.write = Some(do_write);
    ops.utimens = Some(do_utimens);
    ops.fallocate = Some(do_fallocate);
    ops.setxattr = Some(do_setxattr);
    ops.removexattr = Some(do_removexattr);
    ops.fsync = Some(do_fsync);
    // Read ops
    ops.init = Some(xmp_init);
    ops.getattr = Some(xmp_getattr);
    ops.access = Some(xmp_access);
    ops.readlink = Some(xmp_readlink);
    ops.opendir = Some(xmp_opendir);
    ops.readdir = Some(xmp_readdir);
    ops.releasedir = Some(xmp_releasedir);
    ops.open = Some(xmp_open);
    ops.read = Some(xmp_read);
    ops.read_buf = Some(xmp_read_buf);
    ops.statfs = Some(xmp_statfs);
    ops.flush = Some(xmp_flush);
    ops.release = Some(xmp_release);
    ops.getxattr = Some(xmp_getxattr);
    ops.listxattr = Some(xmp_listxattr);
    //ops.ioctl = Some(xmp_ioctl);

    return fuse_main_real(
        argc,
        argv,
        &ops as *const _,
        size_of::<fuse_operations>(),
        ptr::null(),
    );
}
