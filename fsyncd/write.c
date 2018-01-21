#include "codec.h"
#include "defs.h"
#include "fsyncer.h"
#include <stdlib.h>

#include "fsops.h"

int send_op(op_message message, int ret);

static op_message encode_mknod(const char *path, uint32_t mode, uint32_t rdev) {
	NEW_MSG(strlen(path) + 1 + sizeof(mode) + sizeof(rdev), MKNOD);
	ENCODE_STRING(path);
	ENCODE_VALUE(htobe32(mode));
	ENCODE_VALUE(htobe32(rdev));
	return msg;
}

static int do_mknod(const char *path, mode_t mode, dev_t rdev) {
	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, server_path, path);

	int ret = xmp_mknod(real_path, mode, rdev);

	return send_op(encode_mknod(path, mode, rdev), ret);
}

static op_message encode_mkdir(const char *path, uint32_t mode) {
	NEW_MSG(strlen(path) + 1 + sizeof(mode), MKDIR);
	ENCODE_STRING(path);
	ENCODE_VALUE(htobe32(mode));
	return msg;
}

static int do_mkdir(const char *path, mode_t mode) {
	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, server_path, path);

	int ret = xmp_mkdir(real_path, mode);

	return send_op(encode_mkdir(path, mode), ret);
}

op_message encode_unlink(const char *path) {
	NEW_MSG(strlen(path) + 1, UNLINK);
	ENCODE_STRING(path);
	return msg;
}

static int do_unlink(const char *path) {
	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, server_path, path);

	int ret = xmp_unlink(real_path);
	return send_op(encode_unlink(path), ret);
}

static op_message encode_rmdir(const char *path) {
	NEW_MSG(strlen(path) + 1, RMDIR);
	ENCODE_STRING(path);
	return msg;
}

static int do_rmdir(const char *path) {
	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, server_path, path);

	int ret = xmp_rmdir(real_path);
	return send_op(encode_rmdir(path), ret);
}

static op_message encode_symlink(const char *from, const char *to) {
	NEW_MSG(strlen(from) + 1 + strlen(to) + 1, SYMLINK);
	ENCODE_STRING(from);
	ENCODE_STRING(to);
	return msg;
}

static int do_symlink(const char *from, const char *to) {
	char real_from[MAX_PATH_SIZE];
	if (from[0] == '/')
		fake_root(real_from, server_path, from);

	char real_to[MAX_PATH_SIZE];
	fake_root(real_to, server_path, to);

	int ret = xmp_symlink(from[0] == '/' ? real_from : from, real_to);

	return send_op(encode_symlink(from, to), ret);
}

static op_message encode_rename(const char *from, const char *to,
								uint32_t flags) {
	NEW_MSG(strlen(from) + 1 + strlen(to) + 1 + sizeof(flags), RENAME);
	ENCODE_STRING(from);
	ENCODE_STRING(to);
	ENCODE_VALUE(htobe32(flags));
	return msg;
}

static int do_rename(const char *from, const char *to, unsigned int flags) {
	if (flags)
		return -EINVAL;

	/* When we have renameat2() in libc, then we can implement flags */

	char real_from[MAX_PATH_SIZE];
	fake_root(real_from, server_path, from);

	char real_to[MAX_PATH_SIZE];
	fake_root(real_to, server_path, to);

	int ret = xmp_rename(real_from, real_to, flags);

	return send_op(encode_rename(from, to, flags), ret);
}

static op_message encode_link(const char *from, const char *to) {
	NEW_MSG(strlen(from) + 1 + strlen(to) + 1, LINK);
	ENCODE_STRING(from);
	ENCODE_STRING(to);
	return msg;
}

static int do_link(const char *from, const char *to) {
	char real_from[MAX_PATH_SIZE];
	fake_root(real_from, server_path, from);

	char real_to[MAX_PATH_SIZE];
	fake_root(real_to, server_path, to);

	int ret = xmp_link(real_from, real_to);

	return send_op(encode_link(from, to), ret);
}

static op_message encode_chmod(const char *path, uint32_t mode) {
	NEW_MSG(strlen(path) + 1 + sizeof(mode), CHMOD);
	ENCODE_STRING(path);
	ENCODE_VALUE(htobe32(mode));
	return msg;
}

static int do_chmod(const char *path, mode_t mode, struct fuse_file_info *fi) {
	int ret;
	if (fi)
		ret = xmp_chmod(NULL, mode, fi->fh);
	else {
		char real_path[MAX_PATH_SIZE];
		fake_root(real_path, server_path, path);
		ret = xmp_chmod(real_path, mode, -1);
	}

	return send_op(encode_chmod(path, mode), ret);
}

static op_message encode_chown(const char *path, uint32_t uid, uint32_t gid) {
	NEW_MSG(strlen(path) + 1 + sizeof(uid) + sizeof(gid), CHOWN);
	ENCODE_STRING(path);
	ENCODE_VALUE(htobe32(uid));
	ENCODE_VALUE(htobe32(gid));
	return msg;
}

static int do_chown(const char *path, uid_t uid, gid_t gid,
					struct fuse_file_info *fi) {
	int ret;

	if (fi)
		ret = xmp_chown(NULL, uid, gid, fi->fh);
	else {
		char real_path[MAX_PATH_SIZE];
		fake_root(real_path, server_path, path);
		ret = xmp_chown(real_path, uid, gid, -1);
	}

	return send_op(encode_chown(path, uid, gid), ret);
}

static op_message encode_truncate(const char *path, int64_t size) {
	NEW_MSG(strlen(path) + 1 + sizeof(size), TRUNCATE);
	ENCODE_STRING(path);
	ENCODE_VALUE(htobe64(size));
	return msg;
}

static int do_truncate(const char *path, off_t size,
					   struct fuse_file_info *fi) {
	int ret;

	if (fi)
		ret = xmp_truncate(NULL, size, fi->fh);
	else {
		char real_path[MAX_PATH_SIZE];
		fake_root(real_path, server_path, path);
		ret = xmp_truncate(real_path, size, -1);
	}

	return send_op(encode_truncate(path, size), ret);
}

static op_message encode_write(const char *path, const char *buf, uint32_t size,
							   int64_t offset) {
	NEW_MSG(strlen(path) + 1 + size + sizeof(size) + sizeof(offset), WRITE);
	ENCODE_STRING(path);
	ENCODE_OPAQUE(size, buf);
	ENCODE_VALUE(htobe64(offset));
	return msg;
}

static int do_write(const char *path, const char *buf, size_t size,
					off_t offset, struct fuse_file_info *fi) {
	// printf("Write %.*s @ %lu to %s\n", (int)size, buf, offset, path);

	int ret = xmp_write(NULL, buf, size, offset, fi->fh);
	return send_op(encode_write(path, buf, size, offset), ret);
}

	/* Replication for this function is not handled yet.
	static int do_write_buf(const char *path, struct fuse_bufvec *buf,
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

static op_message encode_fallocate(const char *path, int32_t mode,
								   int64_t offset, int64_t length) {
	NEW_MSG(strlen(path) + 1 + sizeof(mode) + sizeof(offset) + sizeof(length),
			FALLOCATE);
	ENCODE_STRING(path);
	ENCODE_VALUE(htobe32(mode));
	ENCODE_VALUE(htobe64(offset));
	ENCODE_VALUE(htobe64(length));
	return msg;
}

static int do_fallocate(const char *path, int mode, off_t offset, off_t length,
						struct fuse_file_info *fi) {
	if (mode)
		return -EOPNOTSUPP;

	int ret = xmp_fallocate(NULL, mode, offset, length, fi->fh);

	return send_op(encode_fallocate(path, mode, offset, length), ret);
}
#endif

#ifdef HAVE_SETXATTR
static op_message encode_setxattr(const char *path, const char *name,
								  const char *value, uint32_t size,
								  int32_t flags) {
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
static int do_setxattr(const char *path, const char *name, const char *value,
					   size_t size, int flags) {
	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, server_path, path);

	int ret = xmp_setxattr(real_path, name, value, size, flags);

	return send_op(encode_setxattr(path, name, value, size, flags), ret);
}

static op_message encode_removexattr(const char *path, const char *name) {
	NEW_MSG(strlen(path) + 1 + strlen(name) + 1, REMOVEXATTR);
	ENCODE_STRING(path);
	ENCODE_STRING(name);
	return msg;
}

static int do_removexattr(const char *path, const char *name) {
	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, server_path, path);

	int ret = xmp_removexattr(real_path, name);

	return send_op(encode_removexattr(path, name), ret);
}
#endif

op_message encode_create(const char *path, uint32_t mode, int32_t flags) {
	NEW_MSG(strlen(path) + 1 + sizeof(mode) + sizeof(flags), CREATE);
	ENCODE_STRING(path);
	ENCODE_VALUE(htobe32(mode));
	ENCODE_VALUE(htobe32(flags));
	return msg;
}

static int do_create(const char *path, mode_t mode, struct fuse_file_info *fi) {
	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, server_path, path);

	int ret = xmp_create(real_path, mode, (int *)&fi->fh, fi->flags);

	// printf("Create %s %d %d\n", real_path, mode, fi->flags);

	return send_op(encode_create(path, mode, fi->flags), ret);
}

#ifdef HAVE_UTIMENSAT
static op_message encode_utimens(const char *path,
								 const struct timespec ts[2]) {
	NEW_MSG(strlen(path) + 1 + (sizeof(struct timespec) * 2), UTIMENS);
	ENCODE_STRING(path);
	// FIXME this is not endian safe, I know.
	ENCODE_FIXED_SIZE((sizeof(struct timespec) * 2), ((const char *)ts));
	return msg;
}

static int do_utimens(const char *path, const struct timespec ts[2],
					  struct fuse_file_info *fi) {
	int ret;

	/* don't use utime/utimes since they follow symlinks */
	if (fi)
		ret = xmp_utimens(NULL, ts, fi->fh);
	else {
		char real_path[MAX_PATH_SIZE];
		fake_root(real_path, server_path, path);
		ret = xmp_utimens(real_path, ts, -1);
	}

	return send_op(encode_utimens(path, ts), ret);
}
#endif

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
#ifdef HAVE_UTIMENSAT
	do_oper->utimens = do_utimens;
#endif
	do_oper->create = do_create;
	do_oper->write = do_write;
#ifdef HAVE_POSIX_FALLOCATE
	do_oper->fallocate = do_fallocate;
#endif
#ifdef HAVE_SETXATTR
	do_oper->setxattr = do_setxattr;
	do_oper->removexattr = do_removexattr;
#endif
}
