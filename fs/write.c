#include "codec.h"
#include "defs.h"
#include "fsyncer.h"
#include <stdlib.h>

int (*send_op)(op_message message);

op_message encode_mknod(const char *path, uint32_t mode, uint32_t rdev) {
	NEW_MSG(strlen(path) + 1 + sizeof(mode) + sizeof(rdev), MKNOD);
	ENCODE_STRING(path);
	ENCODE_VALUE(htobe32(mode));
	ENCODE_VALUE(htobe32(rdev));
	return msg;
}

static int xmp_mknod(const char *path, mode_t mode, dev_t rdev) {
	int res;

	if (send_op(encode_mknod(path, mode, rdev)) < 0)
		;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	if (S_ISFIFO(mode))
		res = mkfifo(real_path, mode);
	else
		res = mknod(real_path, mode, rdev);
	if (res == -1)
		return -errno;

	return 0;
}

op_message encode_mkdir(const char *path, uint32_t mode) {
	NEW_MSG(strlen(path) + 1 + sizeof(mode), MKDIR);
	ENCODE_STRING(path);
	ENCODE_VALUE(htobe32(mode));
	return msg;
}

static int xmp_mkdir(const char *path, mode_t mode) {
	int res;

	if (send_op(encode_mkdir(path, mode)) < 0)
		;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	res = mkdir(real_path, mode);
	if (res == -1)
		return -errno;

	return 0;
}

op_message encode_unlink(const char *path) {
	NEW_MSG(strlen(path) + 1, UNLINK);
	ENCODE_STRING(path);
	return msg;
}

static int xmp_unlink(const char *path) {
	int res;

	if (send_op(encode_unlink(path)) < 0)
		;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	res = unlink(real_path);
	if (res == -1)
		return -errno;

	return 0;
}

op_message encode_rmdir(const char *path) {
	NEW_MSG(strlen(path) + 1, RMDIR);
	ENCODE_STRING(path);
	return msg;
}

static int xmp_rmdir(const char *path) {
	int res;

	if (send_op(encode_rmdir(path)) < 0)
		;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	res = rmdir(real_path);
	if (res == -1)
		return -errno;

	return 0;
}

op_message encode_symlink(const char *from, const char *to) {
	NEW_MSG(strlen(from) + 1 + strlen(to) + 1, SYMLINK);
	ENCODE_STRING(from);
	ENCODE_STRING(to);
	return msg;
}

static int xmp_symlink(const char *from, const char *to) {
	int res;

	if (send_op(encode_symlink(from, to)) < 0)
		;

	char real_from[MAX_PATH_SIZE];
	if (from[0] == '/')
		fake_root(real_from, options.real_path, from);

	char real_to[MAX_PATH_SIZE];
	fake_root(real_to, options.real_path, to);

	res = symlink(from[0] == '/' ? real_from : from, real_to);
	if (res == -1)
		return -errno;

	return 0;
}

op_message encode_rename(const char *from, const char *to, uint32_t flags) {
	NEW_MSG(strlen(from) + 1 + strlen(to) + 1 + sizeof(flags), RENAME);
	ENCODE_STRING(from);
	ENCODE_STRING(to);
	ENCODE_VALUE(htobe32(flags));
	return msg;
}

static int xmp_rename(const char *from, const char *to, unsigned int flags) {
	int res;

	if (send_op(encode_rename(from, to, flags)) < 0)
		;

	/* When we have renameat2() in libc, then we can implement flags */
	if (flags)
		return -EINVAL;

	char real_from[MAX_PATH_SIZE];
	fake_root(real_from, options.real_path, from);

	char real_to[MAX_PATH_SIZE];
	fake_root(real_to, options.real_path, to);

	res = rename(real_from, real_to);
	if (res == -1)
		return -errno;

	return 0;
}

op_message encode_link(const char *from, const char *to) {
	NEW_MSG(strlen(from) + 1 + strlen(to) + 1, LINK);
	ENCODE_STRING(from);
	ENCODE_STRING(to);
	return msg;
}

static int xmp_link(const char *from, const char *to) {
	int res;

	if (send_op(encode_link(from, to)) < 0)
		;

	char real_from[MAX_PATH_SIZE];
	fake_root(real_from, options.real_path, from);

	char real_to[MAX_PATH_SIZE];
	fake_root(real_to, options.real_path, to);

	res = link(real_from, real_to);
	if (res == -1)
		return -errno;

	return 0;
}

op_message encode_chmod(const char *path, uint32_t mode) {
	NEW_MSG(strlen(path) + 1 + sizeof(mode), CHMOD);
	ENCODE_STRING(path);
	ENCODE_VALUE(htobe32(mode));
	return msg;
}

static int xmp_chmod(const char *path, mode_t mode, struct fuse_file_info *fi) {
	int res;

	if (send_op(encode_chmod(path, mode)) < 0)
		;

	if (fi)
		res = fchmod(fi->fh, mode);
	else {
		char real_path[MAX_PATH_SIZE];
		fake_root(real_path, options.real_path, path);
		res = chmod(real_path, mode);
	}

	if (res == -1)
		return -errno;

	return 0;
}

op_message encode_chown(const char *path, uint32_t uid, uint32_t gid) {
	NEW_MSG(strlen(path) + 1 + sizeof(uid) + sizeof(gid), CHOWN);
	ENCODE_STRING(path);
	ENCODE_VALUE(htobe32(uid));
	ENCODE_VALUE(htobe32(gid));
	return msg;
}

static int xmp_chown(const char *path, uid_t uid, gid_t gid,
					 struct fuse_file_info *fi) {
	int res;

	if (send_op(encode_chown(path, uid, gid)) < 0)
		;

	if (fi)
		res = fchown(fi->fh, uid, gid);
	else {
		char real_path[MAX_PATH_SIZE];
		fake_root(real_path, options.real_path, path);
		res = lchown(real_path, uid, gid);
	}

	if (res == -1)
		return -errno;

	return 0;
}

op_message encode_truncate(const char *path, int64_t size) {
	NEW_MSG(strlen(path) + 1 + sizeof(size), TRUNCATE);
	ENCODE_STRING(path);
	ENCODE_VALUE(htobe64(size));
	return msg;
}

int xmp_truncate(const char *path, off_t size, struct fuse_file_info *fi) {
	int res;

	if (send_op(encode_truncate(path, size)) < 0)
		;

	if (fi)
		res = ftruncate(fi->fh, size);
	else {
		char real_path[MAX_PATH_SIZE];
		fake_root(real_path, options.real_path, path);
		res = truncate(real_path, size);
	}

	if (res == -1)
		return -errno;

	return 0;
}

op_message encode_write(const char *path, const char *buf, uint32_t size,
						int64_t offset) {
	NEW_MSG(strlen(path) + 1 + size + sizeof(size) + sizeof(offset), WRITE);
	ENCODE_STRING(path);
	ENCODE_OPAQUE(size, buf);
	ENCODE_VALUE(htobe64(offset));
	return msg;
}

static int xmp_write(const char *path, const char *buf, size_t size,
					 off_t offset, struct fuse_file_info *fi) {
	int res;

	if (send_op(encode_write(path, buf, size, offset)) < 0)
		;

	// printf("Write %.*s @ %lu to %s\n", (int)size, buf, offset, path);

	res = pwrite(fi->fh, buf, size, offset);
	if (res == -1)
		res = -errno;

	return res;
}

	/* Replication for this function is not handled yet.
	static int xmp_write_buf(const char *path, struct fuse_bufvec *buf,
				 off_t offset, struct fuse_file_info *fi)
	{
		struct fuse_bufvec dst = FUSE_BUFVEC_INIT(fuse_buf_size(buf));

		(void) path;

		dst.buf[0].flags = FUSE_BUF_IS_FD | FUSE_BUF_FD_SEEK;
		dst.buf[0].fd = fi->fh;
		dst.buf[0].pos = offset;

		return fuse_buf_copy(&dst, buf, FUSE_BUF_SPLICE_NONBLOCK);
	}
	*/

#ifdef HAVE_POSIX_FALLOCATE

op_message encode_fallocate(const char *path, int32_t mode, int64_t offset,
							int64_t length) {
	NEW_MSG(strlen(path) + 1 + sizeof(mode) + sizeof(offset) + sizeof(length),
			FALLOCATE);
	ENCODE_STRING(path);
	ENCODE_VALUE(htobe32(mode));
	ENCODE_VALUE(htobe64(offset));
	ENCODE_VALUE(htobe64(length));
	return msg;
}

static int xmp_fallocate(const char *path, int mode, off_t offset, off_t length,
						 struct fuse_file_info *fi) {
	if (send_op(encode_fallocate(path, mode, offset, length)) < 0)
		;

	if (mode)
		return -EOPNOTSUPP;

	return -posix_fallocate(fi->fh, offset, length);
}
#endif

#ifdef HAVE_SETXATTR
op_message encode_setxattr(const char *path, const char *name,
						   const char *value, uint32_t size, int32_t flags) {
	NEW_MSG(strlen(path) + 1 + strlen(name) + 1 + size + sizeof(uint32_t) +
				sizeof(flags),
			SETXATTR);
	ENCODE_STRING(path);
	ENCODE_STRING(name);
	ENCODE_OPAQUE(size, value);
	ENCODE_VALUE(htobe32(flags));
	return msg;
}

/* xattr operations are optional and can safely be left unimplemented */
static int xmp_setxattr(const char *path, const char *name, const char *value,
						size_t size, int flags) {

	if (send_op(encode_setxattr(path, name, value, size, flags)) < 0)
		;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	int res = lsetxattr(real_path, name, value, size, flags);
	if (res == -1)
		return -errno;

	return 0;
}

op_message encode_removexattr(const char *path, const char *name) {
	NEW_MSG(strlen(path) + 1 + strlen(name) + 1, REMOVEXATTR);
	ENCODE_STRING(path);
	ENCODE_STRING(name);
	return msg;
}

static int xmp_removexattr(const char *path, const char *name) {

	if (send_op(encode_removexattr(path, name)) < 0)
		;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	int res = lremovexattr(real_path, name);
	if (res == -1)
		return -errno;

	return 0;
}
#endif

op_message encode_create(const char *path, uint32_t mode, int32_t flags) {
	NEW_MSG(strlen(path) + 1 + sizeof(mode) + sizeof(flags), CREATE);
	ENCODE_STRING(path);
	ENCODE_VALUE(htobe32(mode));
	ENCODE_VALUE(htobe32(flags));
	return msg;
}

static int xmp_create(const char *path, mode_t mode,
					  struct fuse_file_info *fi) {
	int fd;

	if (send_op(encode_create(path, mode, fi->flags)) < 0)
		;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	// printf("Create %s %d %d\n", real_path, mode, fi->flags);

	fd = open(real_path, fi->flags, mode);
	if (fd == -1)
		return -errno;

	fi->fh = fd;
	return 0;
}

#ifdef HAVE_UTIMENSAT
op_message encode_utimens(const char *path, const struct timespec ts[2]) {
	NEW_MSG(strlen(path) + 1 + (sizeof(struct timespec) * 2), UTIMENS);
	ENCODE_STRING(path);
	// FIXME this is not endian safe, I know.
	ENCODE_FIXED_SIZE((sizeof(struct timespec) * 2), ((const char *)ts));
	return msg;
}

int xmp_utimens(const char *path, const struct timespec ts[2],
				struct fuse_file_info *fi) {
	int res;

	if (send_op(encode_utimens(path, ts)) < 0)
		;

	/* don't use utime/utimes since they follow symlinks */
	if (fi)
		res = futimens(fi->fh, ts);
	else {
		char real_path[MAX_PATH_SIZE];
		fake_root(real_path, options.real_path, path);
		res = utimensat(0, real_path, ts, AT_SYMLINK_NOFOLLOW);
	}

	if (res == -1)
		return -errno;

	return 0;
}
#endif

void gen_write_ops(struct fuse_operations *xmp_oper) {
	xmp_oper->mknod = xmp_mknod;
	xmp_oper->mkdir = xmp_mkdir;
	xmp_oper->symlink = xmp_symlink;
	xmp_oper->unlink = xmp_unlink;
	xmp_oper->rmdir = xmp_rmdir;
	xmp_oper->rename = xmp_rename;
	xmp_oper->link = xmp_link;
	xmp_oper->chmod = xmp_chmod;
	xmp_oper->chown = xmp_chown;
	xmp_oper->truncate = xmp_truncate;
#ifdef HAVE_UTIMENSAT
	xmp_oper->utimens = xmp_utimens;
#endif
	xmp_oper->create = xmp_create;
	xmp_oper->write = xmp_write;
#ifdef HAVE_POSIX_FALLOCATE
	xmp_oper->fallocate = xmp_fallocate;
#endif
#ifdef HAVE_SETXATTR
	xmp_oper->setxattr = xmp_setxattr;
	xmp_oper->removexattr = xmp_removexattr;
#endif
}
