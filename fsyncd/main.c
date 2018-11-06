#include "fsyncer.h"

int do_mknod(const char *path, mode_t mode, dev_t rdev);
int do_mkdir(const char *path, mode_t mode);
int do_unlink(const char *path);
int do_rmdir(const char *path);
int do_symlink(const char *from, const char *to);
int do_rename(const char *from, const char *to, unsigned int flags);
int do_link(const char *from, const char *to);
int do_chmod(const char *path, mode_t mode, struct fuse_file_info *fi);
int do_chown(const char *path, uid_t uid, gid_t gid,
					struct fuse_file_info *fi);
int do_truncate(const char *path, off_t size,
					   struct fuse_file_info *fi);
int do_write(const char *path, const char *buf, size_t size,
					off_t offset, struct fuse_file_info *fi);
int do_create(const char *path, mode_t mode, struct fuse_file_info *fi);

#ifdef HAVE_POSIX_FALLOCATE
int do_fallocate(const char *path, int mode, off_t offset, off_t length,
						struct fuse_file_info *fi);
#endif

#ifdef HAVE_SETXATTR
/* xattr operations are optional and can safely be left unimplemented */
int do_setxattr(const char *path, const char *name, const char *value,
					   size_t size, int flags);
int do_removexattr(const char *path, const char *name);
#endif

#ifdef HAVE_UTIMENSAT
int do_utimens(const char *path, const struct timespec ts[2],
					  struct fuse_file_info *fi);
#endif

void *xmp_init(struct fuse_conn_info *conn, struct fuse_config *cfg) {
	(void)conn;
	cfg->use_ino = 1;
	// NOTE this makes path NULL to parameters where fi->fh exists. This is evil
	// for the current case of replication. But in future when this is properly
	// handled it can improve performance.
	// refer to
	// https://libfuse.github.io/doxygen/structfuse__config.html#adc93fd1ac03d7f016d6b0bfab77f3863
	// cfg->nullpath_ok = 1;

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
	cfg->auto_cache = 1;
	conn->max_write = 32 * 1024;

	return NULL;
}

void gen_read_ops(struct fuse_operations *xmp_oper);

void gen_write_ops(struct fuse_operations *do_oper) {
	do_oper->mknod = do_mknod;
	do_oper->mkdir = do_mkdir;
	do_oper->symlink = do_symlink;
	do_oper->unlink = do_unlink;
	do_oper->rmdir = do_rmdir;
	do_oper->rename = do_rename;
	do_oper->link = do_link;
	do_oper->chmod = do_chmod;
	do_oper->chown = do_chown;
	do_oper->truncate = do_truncate;
	do_oper->create = do_create;
	do_oper->write = do_write;
#ifdef HAVE_UTIMENSAT
	do_oper->utimens = do_utimens;
#endif
#ifdef HAVE_POSIX_FALLOCATE
	do_oper->fallocate = do_fallocate;
#endif
#ifdef HAVE_SETXATTR
	do_oper->setxattr = do_setxattr;
	do_oper->removexattr = do_removexattr;
#endif
}

int fsyncer_fuse_main(int argc, char **argv) {
	struct fuse_operations xmp_oper = {0};
	gen_read_ops(&xmp_oper);
	gen_write_ops(&xmp_oper);
	return fuse_main(argc, argv, &xmp_oper, NULL);
}