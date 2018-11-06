use libc::*;
use server::fuseops::fuse_operations;
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
    fn gen_read_ops(xmp_oper: *mut fuse_operations);
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

    gen_read_ops(&mut ops as *mut _);

    return fuse_main_real(
        argc,
        argv,
        &ops as *const _,
        size_of::<fuse_operations>(),
        ptr::null(),
    );
}
