#![allow(clippy::cast_lossless)]

use fuse::*;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_int;
use std::os::raw::{c_char, c_uint, c_void};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::{mem, ptr};

struct Inode {
    fd: c_int,
    is_symlink: bool,
    src_dev: dev_t,
    src_ino: ino_t,
    nlookup: Mutex<u64>,
    path: RwLock<Option<(*const Inode, PathBuf)>>,
}

impl Inode {
    // unsafe fn get_path(&self) -> Option<PathBuf> {
    //     let path_lock = self.path.read().unwrap();
    //     let path = path_lock.as_ref()?;
    //     Some(if path.0.is_null() {
    //         PathBuf::from("")
    //     } else {
    //         /* Parent must be a folder, and if it does not have a path, it
    //          * must have been deleted, but it could not be deleted if it has
    //          * children. Therefore, this inode as a child of that folder
    //          * should not have a path, but it does, therefore there is a
    //          * programming error. */
    //         let mut parent_path = (*path.0)
    //             .get_path()
    //             .expect("It is impossible for the parent not to have a
    // path");         parent_path.push(&path.1);
    //         parent_path
    //     })
    // }
    fn get_child(&self, fs: &Fs, name: *const c_char) -> Result<&Inode, i32> {
        let mut attr = unsafe { mem::zeroed() };
        let err = unsafe {
            libc::fstatat(self.fd, name, &mut attr, libc::AT_SYMLINK_NOFOLLOW)
        };
        if err == -1 {
            return Err(errno());
        }
        let inodes = fs.inodes.lock().unwrap();
        // this may be normal for fuse, if you see this, do a lookup now.
        let ino_ptr = &**inodes
            .get(&SrcId(attr.st_ino, attr.st_dev))
            .expect("File was not looked up")
            as *const Inode;
        Ok(unsafe { &*ino_ptr })
    }
}

#[derive(Eq, PartialEq, Hash)]
struct SrcId(ino_t, dev_t);

struct Fs {
    inodes: Mutex<HashMap<SrcId, Box<Inode>>>,
    root: Inode,
    src_dev: dev_t,
    timeout: f64,
}

const EMPTY_PATH: *const c_char = "\0".as_ptr() as *const _;

impl Fs {
    unsafe fn get_inode(&self, ino: fuse_ino_t) -> &Inode {
        if ino == FUSE_ROOT_ID as u64 {
            &self.root
        } else {
            let ino_ptr = ino as *const Inode;
            if (*ino_ptr).fd == -1 {
                panic!("Unknown inode");
            }
            &*ino_ptr
        }
    }
    unsafe fn get_fs_fd(&self, ino: fuse_ino_t) -> c_int {
        self.get_inode(ino).fd
    }
    unsafe fn readdir_common(
        &self,
        req: fuse_req_t,
        ino: fuse_ino_t,
        size: usize,
        off: off_t,
        fi: *mut fuse_file_info,
        plus: bool,
    ) -> FuseReply {
        let dp = (*fi).fh as *mut libc::DIR;
        let inode = self.get_inode(ino);
        let _ = inode.nlookup.lock().unwrap();
        let mut buf: Vec<u8> = Vec::with_capacity(size);
        buf.set_len(size);
        let mut c_size = 0;
        libc::seekdir(dp, off);
        loop {
            let entry = libc::readdir(dp);
            if entry.is_null() {
                if errno() != 0 {
                    return FuseReply::err(errno());
                }
                break;
            }
            if is_dot_or_dotdot((*entry).d_name.as_ptr()) {
                continue;
            }
            c_size += if plus {
                match self.lookup(ino, (*entry).d_name.as_ptr()) {
                    FuseReply::err(e) => return FuseReply::err(e),
                    FuseReply::entry(e) => {
                        let entsize = fuse_add_direntry_plus(
                            req,
                            buf.as_mut_ptr().add(c_size) as _,
                            size - c_size,
                            (*entry).d_name.as_ptr(),
                            &e,
                            (*entry).d_off,
                        );
                        if entsize > size - c_size {
                            self.forget(e.ino, 1);
                            break;
                        }
                        entsize
                    }
                    _ => unreachable!(),
                }
            } else {
                let mut attr: libc::stat = mem::zeroed();
                attr.st_ino = (*entry).d_ino;
                attr.st_mode = ((*entry).d_type as u32) << 12;
                let entsize = fuse_add_direntry(
                    req,
                    buf.as_mut_ptr().add(c_size) as _,
                    size - c_size,
                    (*entry).d_name.as_ptr() as _,
                    &attr,
                    (*entry).d_off,
                );
                if entsize > size - c_size {
                    break;
                }
                entsize
            };
        }
        FuseReply::buf(buf, c_size)
    }
    unsafe fn replace_path(
        &self,
        parent: &Inode,
        name: *const c_char,
        mut newpath: Option<(*const Inode, PathBuf)>,
    ) -> Result<(*const Inode, PathBuf), i32> {
        let ino = match parent.get_child(self, name) {
            Ok(i) => i,
            Err(e) => return Err(e),
        };
        let mut path_lock = ino.path.write().unwrap();
        /* File's primary path could have been deleted, this is a delete
         * call from another path */
        if let Some(primary_path) = path_lock.as_ref() {
            let path_name = Path::new(CStr::from_ptr(name).to_str().unwrap());
            if primary_path.0 == parent as *const Inode
                && primary_path.1 == path_name
            {
                mem::swap(&mut *path_lock, &mut newpath);
                return Ok(newpath.unwrap());
            }
        }
        Err(0)
    }
}

fn errno() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .expect("Failed to get errno")
}

fn self_path(fs_fd: c_int) -> CString {
    CString::new(format!("/proc/self/fd/{}", fs_fd)).unwrap()
}

unsafe fn is_dot_or_dotdot(name: *const c_char) -> bool {
    *name == '.' as i8
        && (*name.add(1) == '\0' as i8
            || (*name.add(1) == '.' as i8 && *name.add(2) == '\0' as i8))
}

impl fuse::FilesystemLL for Fs {
    unsafe fn init(
        &mut self,
        _userdata: *mut c_void,
        _conn: *mut fuse_conn_info,
    ) {
        // if (conn->capable & FUSE_CAP_EXPORT_SUPPORT)
        //     conn->want |= FUSE_CAP_EXPORT_SUPPORT;

        // if (fs.timeout && conn->capable & FUSE_CAP_WRITEBACK_CACHE)
        //     conn->want |= FUSE_CAP_WRITEBACK_CACHE;

        // if (conn->capable & FUSE_CAP_FLOCK_LOCKS)
        //     conn->want |= FUSE_CAP_FLOCK_LOCKS;

        // // Use splicing if supported. Since we are using writeback caching
        // // and readahead, individual requests should have a decent size so
        // // that splicing between fd's is well worth it.
        // if (conn->capable & FUSE_CAP_SPLICE_WRITE && !fs.nosplice)
        //     conn->want |= FUSE_CAP_SPLICE_WRITE;
        // if (conn->capable & FUSE_CAP_SPLICE_READ && !fs.nosplice)
        //     conn->want |= FUSE_CAP_SPLICE_READ;
    }
    unsafe fn release(
        &self,
        _ino: fuse_ino_t,
        fi: *mut fuse_file_info,
    ) -> FuseReply {
        libc::close((*fi).fh as i32);
        FuseReply::err(0)
    }
    unsafe fn forget(&self, ino: fuse_ino_t, nlookup: u64) {
        let inode = self.get_inode(ino);
        let mut ino_lookup = inode.nlookup.lock().unwrap();
        assert!(nlookup <= *ino_lookup, "Negative lookup count");
        *ino_lookup -= nlookup;
        if *ino_lookup == 0 {
            self.inodes
                .lock()
                .unwrap()
                .remove(&SrcId(inode.src_ino, inode.src_dev));
        }
    }
    unsafe fn lookup(
        &self,
        parent: fuse_ino_t,
        name: *const c_char,
    ) -> FuseReply {
        let mut e: fuse_entry_param = mem::zeroed();
        e.attr_timeout = self.timeout;
        e.entry_timeout = self.timeout;

        let newfd = libc::openat(
            self.get_fs_fd(parent),
            name,
            libc::O_PATH | libc::O_NOFOLLOW,
        );
        if newfd == -1 {
            return FuseReply::err(errno());
        }
        let err = libc::fstatat(
            newfd,
            EMPTY_PATH,
            &mut e.attr,
            libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
        );
        if err == -1 {
            return FuseReply::err(errno());
        }

        if e.attr.st_dev != self.src_dev {
            // Mountpoints not supported, not entirely sure why, I guess they
            // are too lazy for lookups?
            eprintln!(
                "WARNING: Mountpoints in the source directory tree will be \
                 hidden."
            );
            return FuseReply::err(libc::ENOTSUP);
        }

        if e.attr.st_ino == FUSE_ROOT_ID as u64 {
            eprintln!(
                "ERROR: Source directory tree must not include inode {}",
                FUSE_ROOT_ID
            );
            return FuseReply::err(libc::EIO);
        }
        {
            let mut inodes = self.inodes.lock().unwrap();
            let inode = inodes
                .entry(SrcId(e.attr.st_ino, e.attr.st_dev))
                .or_insert_with(|| {
                    let name =
                        PathBuf::from(CStr::from_ptr(name).to_str().unwrap());
                    Box::new(Inode {
                        src_ino: e.attr.st_ino,
                        src_dev: e.attr.st_dev,
                        is_symlink: e.attr.st_mode & libc::S_IFLNK == S_IFLNK,
                        nlookup: Mutex::new(1),
                        fd: newfd,
                        path: RwLock::new(Some((parent as *const Inode, name))),
                    })
                });
            *inode.nlookup.lock().unwrap() += 1;
            libc::close(newfd);
            e.ino = &**inode as *const Inode as _;
        }
        FuseReply::entry(e)
    }
    /* ===================== READS ===================== */
    unsafe fn read(
        &self,
        _ino: fuse_ino_t,
        size: usize,
        off: off_t,
        fi: *mut fuse_file_info,
    ) -> FuseReply {
        let mut buf = FUSE_BUFVEC_INIT!(size);
        buf.buf[0].flags =
            fuse_buf_flags_FUSE_BUF_IS_FD | fuse_buf_flags_FUSE_BUF_FD_SEEK;
        buf.buf[0].fd = (*fi).fh as i32;
        buf.buf[0].pos = off;
        FuseReply::data(buf, 0)
    }

    unsafe fn fsync(
        &self,
        _ino: fuse_ino_t,
        datasync: c_int,
        fi: *mut fuse_file_info,
    ) -> FuseReply {
        let res = if datasync != 0 {
            libc::fdatasync((*fi).fh as i32)
        } else {
            libc::fsync((*fi).fh as i32)
        };
        FuseReply::err(if res == -1 { errno() } else { 0 })
    }
    unsafe fn flush(
        &self,
        _ino: fuse_ino_t,
        fi: *mut fuse_file_info,
    ) -> FuseReply {
        FuseReply::err(if libc::close(libc::dup((*fi).fh as i32)) == -1 {
            errno()
        } else {
            0
        })
    }
    unsafe fn statfs(&self, ino: fuse_ino_t) -> FuseReply {
        let mut stbuf: statvfs = mem::zeroed();
        let res = fstatvfs(self.get_fs_fd(ino), &mut stbuf);
        if res < 0 {
            FuseReply::err(-res as i32)
        } else {
            FuseReply::statfs(stbuf)
        }
    }
    unsafe fn getattr(
        &self,
        ino: fuse_ino_t,
        _fi: *mut fuse_file_info,
    ) -> FuseReply {
        let ino = self.get_inode(ino);
        let mut attr: libc::stat = mem::zeroed();
        let res = libc::fstatat(
            ino.fd,
            EMPTY_PATH,
            &mut attr,
            libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
        );
        if res < 0 {
            FuseReply::err(errno())
        } else {
            FuseReply::attr(attr, self.timeout)
        }
    }
    unsafe fn readlink(&self, ino: fuse_ino_t) -> FuseReply {
        const PATH_BUF_SIZE: usize = libc::PATH_MAX as usize + 1;
        let mut buf: Vec<u8> = Vec::with_capacity(PATH_BUF_SIZE);
        buf.set_len(PATH_BUF_SIZE);
        let res = libc::readlinkat(
            self.get_fs_fd(ino),
            EMPTY_PATH,
            buf.as_mut_ptr() as *mut _,
            PATH_BUF_SIZE,
        );
        if res == -1 {
            return FuseReply::err(errno());
        }
        buf[res as usize] = 0;
        buf.set_len(res as usize + 1);
        FuseReply::readlink(CString::new(buf).unwrap())
    }
    unsafe fn open(
        &self,
        ino: fuse_ino_t,
        fi: *mut fuse_file_info,
    ) -> FuseReply {
        if self.timeout != 0.0
            && ((*fi).flags & libc::O_ACCMODE) == libc::O_WRONLY
        {
            (*fi).flags &= !libc::O_ACCMODE;
            (*fi).flags |= libc::O_RDWR;
        }
        if self.timeout != 0.0 {
            (*fi).flags &= !libc::O_APPEND;
        }
        let fd = libc::open(
            self_path(self.get_fs_fd(ino)).as_ptr(),
            (*fi).flags & !libc::O_NOFOLLOW,
        );
        if fd == -1 {
            return FuseReply::err(errno());
        }
        (*fi).set_keep_cache(if self.timeout != 0.0 { 1 } else { 0 });
        (*fi).fh = fd as u64;
        FuseReply::open(fi)
    }
    unsafe fn opendir(
        &self,
        ino: fuse_ino_t,
        fi: *mut fuse_file_info,
    ) -> FuseReply {
        let fd = libc::openat(
            self.get_fs_fd(ino),
            ".\0".as_ptr() as _,
            libc::O_RDONLY,
        );
        if fd == -1 {
            return FuseReply::err(errno());
        }
        let dp = libc::fdopendir(fd);
        if dp.is_null() {
            return FuseReply::err(errno());
        }
        (*fi).fh = dp as u64;
        if self.timeout != 0.0 {
            (*fi).set_keep_cache(1);
            (*fi).set_cache_readdir(1);
        }
        FuseReply::open(fi)
    }
    unsafe fn releasedir(
        &self,
        _ino: fuse_ino_t,
        fi: *mut fuse_file_info,
    ) -> FuseReply {
        libc::closedir((*fi).fh as *mut libc::DIR);
        FuseReply::err(0)
    }
    unsafe fn readdirplus(
        &self,
        req: fuse_req_t,
        ino: fuse_ino_t,
        size: usize,
        off: off_t,
        fi: *mut fuse_file_info,
    ) -> FuseReply {
        self.readdir_common(req, ino, size, off, fi, true)
    }
    unsafe fn readdir(
        &self,
        req: fuse_req_t,
        ino: fuse_ino_t,
        size: usize,
        off: off_t,
        fi: *mut fuse_file_info,
    ) -> FuseReply {
        self.readdir_common(req, ino, size, off, fi, false)
    }
    unsafe fn fsyncdir(
        &self,
        _ino: fuse_ino_t,
        datasync: c_int,
        fi: *mut fuse_file_info,
    ) -> FuseReply {
        let fd = libc::dirfd((*fi).fh as *mut libc::DIR);
        let res = if datasync != 0 {
            libc::fdatasync(fd)
        } else {
            libc::fsync(fd)
        };
        FuseReply::err(if res == -1 { errno() } else { 0 })
    }
    unsafe fn listxattr(&self, ino: fuse_ino_t, size: usize) -> FuseReply {
        let inode = self.get_inode(ino);
        if inode.is_symlink {
            return FuseReply::err(libc::ENOTSUP);
        }
        let procname = self_path(inode.fd);
        if size != 0 {
            let mut value: Vec<u8> = Vec::with_capacity(size);
            value.set_len(size);
            let ret = libc::listxattr(
                procname.as_ptr(),
                value.as_mut_ptr() as _,
                size,
            );
            if ret == -1 {
                return FuseReply::err(errno());
            }
            if ret == 0 {
                return FuseReply::err(0);
            }
            FuseReply::buf(value, ret as usize)
        } else {
            let ret = libc::listxattr(procname.as_ptr(), ptr::null_mut(), 0);
            if ret == -1 {
                return FuseReply::err(errno());
            }
            FuseReply::xattr(ret as usize)
        }
    }
    unsafe fn getxattr(
        &self,
        ino: fuse_ino_t,
        name: *const c_char,
        size: usize,
    ) -> FuseReply {
        let inode = self.get_inode(ino);
        if inode.is_symlink {
            return FuseReply::err(libc::ENOTSUP);
        }
        let procname = self_path(inode.fd);
        if size != 0 {
            let mut value: Vec<u8> = Vec::with_capacity(size);
            value.set_len(size);
            let ret = libc::getxattr(
                procname.as_ptr(),
                name,
                value.as_mut_ptr() as _,
                size,
            );
            if ret == -1 {
                return FuseReply::err(errno());
            }
            if ret == 0 {
                return FuseReply::err(0);
            }
            FuseReply::buf(value, ret as usize)
        } else {
            let ret =
                libc::getxattr(procname.as_ptr(), name, ptr::null_mut(), 0);
            if ret == -1 {
                return FuseReply::err(errno());
            }
            FuseReply::xattr(ret as usize)
        }
    }
    /* ===================== WRITES ===================== */
    unsafe fn write_buf(
        &self,
        _ino: fuse_ino_t,
        bufv: *mut fuse_bufvec,
        off: off_t,
        fi: *mut fuse_file_info,
    ) -> FuseReply {
        let size = fuse_buf_size(bufv);
        let mut out_buf = FUSE_BUFVEC_INIT!(size);
        out_buf.buf[0].flags =
            fuse_buf_flags_FUSE_BUF_IS_FD | fuse_buf_flags_FUSE_BUF_FD_SEEK;
        out_buf.buf[0].fd = (*fi).fh as i32;
        out_buf.buf[0].pos = off;

        let res = fuse_buf_copy(&mut out_buf, bufv, 0);
        if res < 0 {
            FuseReply::err(-res as i32)
        } else {
            FuseReply::write(res as usize)
        }
    }
    unsafe fn fallocate(
        &self,
        _ino: fuse_ino_t,
        mode: c_int,
        offset: off_t,
        length: off_t,
        fi: *mut fuse_file_info,
    ) -> FuseReply {
        if mode != 0 {
            return FuseReply::err(libc::EOPNOTSUPP);
        }
        FuseReply::err(posix_fallocate((*fi).fh as i32, offset, length))
    }
    unsafe fn mknod(
        &self,
        parent: fuse_ino_t,
        name: *const c_char,
        mode: mode_t,
        rdev: dev_t,
    ) -> FuseReply {
        let res = libc::mknodat(self.get_fs_fd(parent), name, mode, rdev);
        if res < 0 {
            return FuseReply::err(errno());
        }
        self.lookup(parent, name)
    }
    unsafe fn mkdir(
        &self,
        parent: fuse_ino_t,
        name: *const c_char,
        mode: mode_t,
    ) -> FuseReply {
        let res = libc::mkdirat(self.get_fs_fd(parent), name, mode);
        if res < 0 {
            return FuseReply::err(errno());
        }
        self.lookup(parent, name)
    }
    unsafe fn symlink(
        &self,
        link: *const c_char,
        parent: fuse_ino_t,
        name: *const c_char,
    ) -> FuseReply {
        let res = libc::symlinkat(link, self.get_fs_fd(parent), name);
        if res < 0 {
            return FuseReply::err(errno());
        }
        self.lookup(parent, name)
    }
    unsafe fn create(
        &self,
        parent: fuse_ino_t,
        name: *const c_char,
        mode: mode_t,
        fi: *mut fuse_file_info,
    ) -> FuseReply {
        let fd = libc::openat(
            self.get_fs_fd(parent),
            name,
            ((*fi).flags | libc::O_CREAT) & !libc::O_NOFOLLOW,
            mode,
        );
        if fd == -1 {
            return FuseReply::err(errno());
        }
        (*fi).fh = fd as u64;
        match self.lookup(parent, name) {
            FuseReply::err(e) => FuseReply::err(e),
            FuseReply::entry(e) => FuseReply::create(e, fi),
            _ => unreachable!(),
        }
    }
    unsafe fn link(
        &self,
        ino: fuse_ino_t,
        newparent: fuse_ino_t,
        newname: *const c_char,
    ) -> FuseReply {
        let inode = self.get_inode(ino);
        let p_fd = self.get_fs_fd(newparent);
        let mut e: fuse_entry_param = mem::zeroed();
        e.attr_timeout = self.timeout;
        e.entry_timeout = self.timeout;
        let res = if inode.is_symlink {
            let res = libc::linkat(
                inode.fd,
                EMPTY_PATH,
                p_fd,
                newname,
                libc::AT_EMPTY_PATH,
            );
            let errno = errno();
            if res == -1 && (errno == libc::ENOENT || errno == libc::EINVAL) {
                return FuseReply::err(libc::EOPNOTSUPP);
            }
            res
        } else {
            libc::linkat(
                AT_FDCWD,
                self_path(inode.fd).as_ptr(),
                p_fd,
                newname,
                libc::AT_SYMLINK_FOLLOW,
            )
        };
        if res == -1 {
            return FuseReply::err(errno());
        }
        let res = libc::fstatat(
            inode.fd,
            EMPTY_PATH,
            &mut e.attr,
            libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
        );
        if res == -1 {
            return FuseReply::err(errno());
        }
        e.ino = ino;
        *inode.nlookup.lock().unwrap() += 1;
        FuseReply::entry(e)
    }
    unsafe fn setxattr(
        &self,
        ino: fuse_ino_t,
        name: *const c_char,
        value: *const c_char,
        size: usize,
        flags: c_int,
    ) -> FuseReply {
        let inode = self.get_inode(ino);
        if inode.is_symlink {
            return FuseReply::err(libc::ENOTSUP);
        }
        if libc::setxattr(
            self_path(inode.fd).as_ptr(),
            name,
            value as _,
            size,
            flags,
        ) == -1
        {
            FuseReply::err(errno())
        } else {
            FuseReply::err(0)
        }
    }
    unsafe fn removexattr(
        &self,
        ino: fuse_ino_t,
        name: *const c_char,
    ) -> FuseReply {
        let inode = self.get_inode(ino);
        if inode.is_symlink {
            return FuseReply::err(libc::ENOTSUP);
        }
        if libc::removexattr(self_path(inode.fd).as_ptr(), name) == -1 {
            FuseReply::err(errno())
        } else {
            FuseReply::err(0)
        }
    }
    unsafe fn chmod(
        &self,
        ino: fuse_ino_t,
        mode: mode_t,
        fi: *mut fuse_file_info,
    ) -> FuseReply {
        let res = if !fi.is_null() {
            libc::fchmod((*fi).fh as i32, mode)
        } else {
            libc::chmod(self_path(self.get_fs_fd(ino)).as_ptr(), mode)
        };
        FuseReply::err(if res == -1 { errno() } else { 0 })
    }
    unsafe fn chown(
        &self,
        ino: fuse_ino_t,
        uid: uid_t,
        gid: gid_t,
        _fi: *mut fuse_file_info,
    ) -> FuseReply {
        FuseReply::err(
            if libc::fchownat(
                self.get_fs_fd(ino),
                EMPTY_PATH,
                uid,
                gid,
                libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
            ) == -1
            {
                errno()
            } else {
                0
            },
        )
    }
    unsafe fn truncate(
        &self,
        ino: fuse_ino_t,
        size: off_t,
        fi: *mut fuse_file_info,
    ) -> FuseReply {
        let res = if !fi.is_null() {
            libc::ftruncate((*fi).fh as i32, size)
        } else {
            libc::truncate(self_path(self.get_fs_fd(ino)).as_ptr(), size)
        };
        FuseReply::err(if res == -1 { errno() } else { 0 })
    }
    unsafe fn utimens(
        &self,
        ino: fuse_ino_t,
        ts: *const libc::timespec,
        fi: *mut fuse_file_info,
    ) -> FuseReply {
        let res = if !fi.is_null() {
            libc::futimens((*fi).fh as i32, ts)
        } else {
            let inode = self.get_inode(ino);
            if inode.is_symlink {
                let res = libc::utimensat(
                    inode.fd,
                    EMPTY_PATH,
                    ts,
                    libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
                );
                if res == -1 && errno() == libc::EINVAL {
                    return FuseReply::err(libc::EPERM);
                }
                res
            } else {
                utimensat(libc::AT_FDCWD, self_path(inode.fd).as_ptr(), ts, 0)
            }
        };
        FuseReply::err(if res == -1 { errno() } else { 0 })
    }
    /* ===================== PATH CHANGERS ===================== */
    unsafe fn rename(
        &self,
        parent: fuse_ino_t,
        name: *const c_char,
        newparent: fuse_ino_t,
        newname: *const c_char,
        flags: c_uint,
    ) -> FuseReply {
        // FIXME I probably need to hold both locks for the entire op
        if flags != 0 {
            return FuseReply::err(libc::EINVAL);
        }
        // Change the path of the inode being renamed
        let ino_op = self.get_inode(parent);

        match self.replace_path(
            ino_op,
            name,
            Some((
                newparent as *const Inode,
                PathBuf::from(CStr::from_ptr(name).to_str().unwrap()),
            )),
        ) {
            Err(e) if e != 0 => return FuseReply::err(e),
            _ => {}
        }

        // Delete the path of the inode that is being overwritten
        let ino_np = self.get_inode(parent);
        match self.replace_path(ino_np, newname, None) {
            // If the newname does not exist it is not a problem.
            Err(e) if e != 0 && e != libc::ENOENT => return FuseReply::err(e),
            _ => {}
        }
        let res = libc::renameat(ino_op.fd, name, ino_np.fd, newname);
        FuseReply::err(if res == -1 { errno() } else { 0 })
    }
    unsafe fn unlink(
        &self,
        parent: fuse_ino_t,
        name: *const c_char,
    ) -> FuseReply {
        // FIXME I probably need to hold  path_lock during this entire operation
        let ino_p = self.get_inode(parent);
        match self.replace_path(ino_p, name, None) {
            Err(e) if e != 0 => return FuseReply::err(e),
            _ => {}
        };
        let res = libc::unlinkat(ino_p.fd, name, 0);
        FuseReply::err(if res == -1 { errno() } else { 0 })
    }
    unsafe fn rmdir(
        &self,
        parent: fuse_ino_t,
        name: *const c_char,
    ) -> FuseReply {
        // FIXME I probably need to hold  path_lock during this entire operation
        let ino_p = self.get_inode(parent);
        /* It must have a path, otherwise there is nothing to unlink it from.
         * Directories cannot be hard linked so its primary path is the only
         * possible path */
        match self.replace_path(ino_p, name, None) {
            Ok(_) => {}
            Err(0) => panic!("It must have a path"),
            Err(e) => return FuseReply::err(e),
        }
        let res = libc::unlinkat(ino_p.fd, name, libc::AT_REMOVEDIR);
        FuseReply::err(if res == -1 { errno() } else { 0 })
    }
}
