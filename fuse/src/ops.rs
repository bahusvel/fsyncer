#[macro_export]
macro_rules! op_args {
    (init, $callback:ident) => {
        $callback!(init, userdata: *mut c_void, conn: *mut fuse_conn_info);
    };
    (destroy, $callback:ident) => {
        $callback!(destroy, userdata: *mut c_void);
    };
    (forget, $callback:ident) => {
        $callback!(forget, ino: fuse_ino_t, nlookup: u64);
    };
    (forget_multi, $callback:ident) => {
        $callback!(forget_multi, count: usize, forgets: *mut fuse_forget_data);
    };
    (readdir, $callback:ident) => {
        $callback!(
            readdir,
            req: fuse_req_t,
            ino: fuse_ino_t,
            size: usize,
            off: off_t,
            fi: *mut fuse_file_info
        );
    };
    (readdirplus, $callback:ident) => {
        $callback!(
            readdirplus,
            req: fuse_req_t,
            ino: fuse_ino_t,
            size: usize,
            off: off_t,
            fi: *mut fuse_file_info
        );
    };
    (poll, $callback:ident) => {
        $callback!(
            poll,
            req: fuse_req_t,
            ino: fuse_ino_t,
            fi: *mut fuse_file_info,
            ph: *mut fuse_pollhandle
        );
    };
    (chmod, $callback:ident) => {
        $callback!(
            chmod,
            ino: fuse_ino_t,
            mode: mode_t,
            fi: *mut fuse_file_info
        );
    };
    (chown, $callback:ident) => {
        $callback!(
            chown,
            ino: fuse_ino_t,
            uid: uid_t,
            gid: gid_t,
            fi: *mut fuse_file_info
        );
    };
    (truncate, $callback:ident) => {
        $callback!(
            truncate,
            ino: fuse_ino_t,
            size: off_t,
            fi: *mut fuse_file_info
        );
    };
    (utimens, $callback:ident) => {
        $callback!(
            utimens,
            ino: fuse_ino_t,
            ts: *const timespec,
            fi: *mut fuse_file_info
        );
    };
    (setattr, $callback:ident) => {
        $callback!(
            setattr,
            ino: fuse_ino_t,
            attr: *mut stat,
            to_set: c_int,
            fi: *mut fuse_file_info
        );
    };
    (lookup, $callback:ident) => {
        $callback!(lookup, parent: fuse_ino_t, name: *const c_char);
    };
    (getattr, $callback:ident) => {
        $callback!(getattr, ino: fuse_ino_t, fi: *mut fuse_file_info);
    };
    (readlink, $callback:ident) => {
        $callback!(readlink, ino: fuse_ino_t);
    };
    (mknod, $callback:ident) => {
        $callback!(
            mknod,
            parent: fuse_ino_t,
            name: *const c_char,
            mode: mode_t,
            rdev: dev_t
        );
    };
    (mkdir, $callback:ident) => {
        $callback!(
            mkdir,
            parent: fuse_ino_t,
            name: *const c_char,
            mode: mode_t
        );
    };
    (unlink, $callback:ident) => {
        $callback!(unlink, parent: fuse_ino_t, name: *const c_char);
    };
    (rmdir, $callback:ident) => {
        $callback!(rmdir, parent: fuse_ino_t, name: *const c_char);
    };
    (symlink, $callback:ident) => {
        $callback!(
            symlink,
            link: *const c_char,
            parent: fuse_ino_t,
            name: *const c_char
        );
    };
    (rename, $callback:ident) => {
        $callback!(
            rename,
            parent: fuse_ino_t,
            name: *const c_char,
            newparent: fuse_ino_t,
            newname: *const c_char,
            flags: c_uint
        );
    };
    (link, $callback:ident) => {
        $callback!(
            link,
            ino: fuse_ino_t,
            newparent: fuse_ino_t,
            newname: *const c_char
        );
    };
    (open, $callback:ident) => {
        $callback!(open, ino: fuse_ino_t, fi: *mut fuse_file_info);
    };
    (read, $callback:ident) => {
        $callback!(
            read,
            ino: fuse_ino_t,
            size: usize,
            off: off_t,
            fi: *mut fuse_file_info
        );
    };
    (write, $callback:ident) => {
        $callback!(
            write,
            ino: fuse_ino_t,
            buf: *const c_char,
            size: usize,
            off: off_t,
            fi: *mut fuse_file_info
        );
    };
    (flush, $callback:ident) => {
        $callback!(flush, ino: fuse_ino_t, fi: *mut fuse_file_info);
    };
    (release, $callback:ident) => {
        $callback!(release, ino: fuse_ino_t, fi: *mut fuse_file_info);
    };
    (fsync, $callback:ident) => {
        $callback!(
            fsync,
            ino: fuse_ino_t,
            datasync: c_int,
            fi: *mut fuse_file_info
        );
    };
    (fsyncdir, $callback:ident) => {
        $callback!(
            fsyncdir,
            ino: fuse_ino_t,
            datasync: c_int,
            fi: *mut fuse_file_info
        );
    };
    (opendir, $callback:ident) => {
        $callback!(opendir, ino: fuse_ino_t, fi: *mut fuse_file_info);
    };
    (releasedir, $callback:ident) => {
        $callback!(releasedir, ino: fuse_ino_t, fi: *mut fuse_file_info);
    };
    (statfs, $callback:ident) => {
        $callback!(statfs, ino: fuse_ino_t);
    };
    (setxattr, $callback:ident) => {
        $callback!(
            setxattr,
            ino: fuse_ino_t,
            name: *const c_char,
            value: *const c_char,
            size: usize,
            flags: c_int
        );
    };
    (getxattr, $callback:ident) => {
        $callback!(getxattr, ino: fuse_ino_t, name: *const c_char, size: usize);
    };
    (listxattr, $callback:ident) => {
        $callback!(listxattr, ino: fuse_ino_t, size: usize);
    };
    (removexattr, $callback:ident) => {
        $callback!(removexattr, ino: fuse_ino_t, name: *const c_char);
    };
    (access, $callback:ident) => {
        $callback!(access, ino: fuse_ino_t, mask: c_int);
    };
    (ioctl, $callback:ident) => {
        $callback!(
            ioctl,
            ino: fuse_ino_t,
            cmd: c_uint,
            arg: *mut c_void,
            fi: *mut fuse_file_info,
            flags: c_uint,
            in_buf: *const c_void,
            in_bufsz: usize,
            out_bufsz: usize
        );
    };
    (retrieve_reply, $callback:ident) => {
        $callback!(
            retrieve_reply,
            cookie: *mut c_void,
            ino: fuse_ino_t,
            offset: off_t,
            bufv: *mut fuse_bufvec
        );
    };

    (create, $callback:ident) => {
        $callback!(
            create,
            parent: fuse_ino_t,
            name: *const c_char,
            mode: mode_t,
            fi: *mut fuse_file_info
        );
    };
    (getlk, $callback:ident) => {
        $callback!(
            getlk,
            ino: fuse_ino_t,
            fi: *mut fuse_file_info,
            lock: *mut flock
        );
    };
    (setlk, $callback:ident) => {
        $callback!(
            setlk,
            ino: fuse_ino_t,
            fi: *mut fuse_file_info,
            lock: *mut flock,
            sleep: c_int
        );
    };
    (bmap, $callback:ident) => {
        $callback!(bmap, ino: fuse_ino_t, blocksize: usize, idx: u64);
    };
    (write_buf, $callback:ident) => {
        $callback!(
            write_buf,
            ino: fuse_ino_t,
            bufv: *mut fuse_bufvec,
            off: off_t,
            fi: *mut fuse_file_info
        );
    };
    (flock, $callback:ident) => {
        $callback!(flock, ino: fuse_ino_t, fi: *mut fuse_file_info, op: c_int);
    };
    (fallocate, $callback:ident) => {
        $callback!(
            fallocate,
            ino: fuse_ino_t,
            mode: c_int,
            offset: off_t,
            length: off_t,
            fi: *mut fuse_file_info
        );
    };
    (copy_file_range, $callback:ident) => {
        $callback!(
            copy_file_range,
            ino_in: fuse_ino_t,
            off_in: off_t,
            fi_in: *mut fuse_file_info,
            ino_out: fuse_ino_t,
            off_out: off_t,
            fi_out: *mut fuse_file_info,
            len: usize,
            flags: c_int
        );
    };
}

#[macro_export]
macro_rules! op_list {
    (generic, $callback:ident$(,$arg:tt),*) => {
        $callback!(lookup $(,$arg)*);
        $callback!(getattr $(,$arg)*);
        $callback!(readlink $(,$arg)*);
        $callback!(mknod $(,$arg)*);
        $callback!(mkdir $(,$arg)*);
        $callback!(unlink $(,$arg)*);
        $callback!(rmdir $(,$arg)*);
        $callback!(symlink $(,$arg)*);
        $callback!(rename $(,$arg)*);
        $callback!(link $(,$arg)*);
        $callback!(open $(,$arg)*);
        $callback!(read $(,$arg)*);
        $callback!(write $(,$arg)*);
        $callback!(flush $(,$arg)*);
        $callback!(release $(,$arg)*);
        $callback!(fsync $(,$arg)*);
        $callback!(fsyncdir $(,$arg)*);
        $callback!(opendir $(,$arg)*);
        $callback!(releasedir $(,$arg)*);
        $callback!(statfs $(,$arg)*);
        $callback!(setxattr $(,$arg)*);
        $callback!(getxattr $(,$arg)*);
        $callback!(listxattr $(,$arg)*);
        $callback!(removexattr $(,$arg)*);
        $callback!(access $(,$arg)*);
        $callback!(ioctl $(,$arg)*);
        $callback!(retrieve_reply $(,$arg)*);
        $callback!(create $(,$arg)*);
        $callback!(getlk $(,$arg)*);
        $callback!(setlk $(,$arg)*);
        $callback!(bmap $(,$arg)*);
        $callback!(write_buf $(,$arg)*);
        $callback!(flock $(,$arg)*);
        $callback!(fallocate $(,$arg)*);
        $callback!(copy_file_range $(,$arg)*);
    };
    (withreq, $callback:ident$(,$arg:tt),*) => {
        $callback!(readdir $(,$arg)*);
        $callback!(readdirplus $(,$arg)*);
        $callback!(poll $(,$arg)*);
    };
    (fsmut, $callback:ident$(,$arg:tt),*) => {
        $callback!(init $(,$arg)*);
        $callback!(destroy $(,$arg)*);
    };
    (nonereply, $callback:ident$(,$arg:tt),*) => {
        $callback!(forget $(,$arg)*);
        $callback!(forget_multi $(,$arg)*);
    };
    (attr, $callback:ident$(,$arg:tt),*) => {
        $callback!(chmod $(,$arg)*);
        $callback!(chown $(,$arg)*);
        $callback!(truncate $(,$arg)*);
        $callback!(utimens $(,$arg)*);
    };
    (all, $callback:ident $(,$arg:tt)*) => {
        op_list!(fsmut, $callback $(,$arg)*);
        op_list!(nonereply, $callback $(,$arg)*);
        op_list!(withreq, $callback $(,$arg)*);
        op_list!(generic, $callback $(,$arg)*);
    };
}
