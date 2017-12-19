#include "defs.h"
#include "ops.h"
#include <stdlib.h>

#define ENCODE_STRING(str)                                                     \
	memcpy(msg_data, str, strlen(str) + 1);                                    \
	msg_data += strlen(str) + 1;
#define ENCODE_VALUE(val)                                                      \
	*(typeof(val) *)(msg_data) = val;                                          \
	msg_data += sizeof(val);
#define ENCODE_OPAQUE(size, buf)                                               \
	ENCODE_VALUE(htobe32(size));                                               \
	memcpy(msg_data, buf, size);                                               \
	msg_data += size;

#define NEW_MSG(size, type)                                                    \
	size_t tmp_size = (size) + sizeof(struct op_msg);                          \
	op_message msg = malloc(tmp_size);                                         \
	msg->op_type = type;                                                       \
	msg->op_length = tmp_size;                                                 \
	unsigned char *msg_data = msg->data;

int send_op(op_message message);

static int fake_root(char *dest, const char *root_path, const char *path) {
	if ((strlen(root_path) + strlen(path)) > MAX_PATH_SIZE) {
		return -1;
	}
	strcpy(dest, root_path);
	strcat(dest, path);
	return 0;
}

op_message encode_mknod(const char *path, uint32_t mode, uint32_t rdev) {
	NEW_MSG(strlen(path) + 1 + sizeof(mode) + sizeof(rdev), MKNOD);
	ENCODE_STRING(path);
	ENCODE_VALUE(htobe32(mode));
	ENCODE_VALUE(htobe32(rdev));
	return msg;
}

int xmp_mknod(const char *path, mode_t mode, dev_t rdev) {
	int res;

	if (send_op(encode_mknod(path, mode, rdev)) < 0)
		;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	/* On Linux this could just be 'mknod(path, mode, rdev)' but this
	   is more portable */
	if (S_ISREG(mode)) {
		res = open(real_path, O_CREAT | O_EXCL | O_WRONLY, mode);
		if (res >= 0)
			res = close(res);
	} else if (S_ISFIFO(mode))
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

int xmp_mkdir(const char *path, mode_t mode) {
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

int xmp_unlink(const char *path) {
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
	NEW_MSG(strlen(path), RMDIR);
	ENCODE_STRING(path);
	return msg;
}

int xmp_rmdir(const char *path) {
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

int xmp_symlink(const char *from, const char *to) {
	int res;

	if (send_op(encode_symlink(from, to)) < 0)
		;

	char real_from[MAX_PATH_SIZE];
	fake_root(real_from, options.real_path, from);

	char real_to[MAX_PATH_SIZE];
	fake_root(real_to, options.real_path, to);

	res = symlink(real_from, real_to);
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

int xmp_rename(const char *from, const char *to, unsigned int flags) {
	int res;

	if (send_op(encode_rename(from, to, flags)) < 0)
		;

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

int xmp_link(const char *from, const char *to) {
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

int xmp_chmod(const char *path, mode_t mode, struct fuse_file_info *fi) {
	(void)fi;
	int res;

	if (send_op(encode_chmod(path, mode)) < 0)
		;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	res = chmod(real_path, mode);
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

int xmp_chown(const char *path, uid_t uid, gid_t gid,
			  struct fuse_file_info *fi) {
	(void)fi;
	int res;

	if (send_op(encode_chown(path, uid, gid)) < 0)
		;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	res = lchown(real_path, uid, gid);
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
	(void)fi;
	int res;

	if (send_op(encode_truncate(path, size)) < 0)
		;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	res = truncate(real_path, size);
	if (res == -1)
		return -errno;

	return 0;
}

op_message encode_write(const char *path, const char *buf, uint64_t size,
						int64_t offset) {
	NEW_MSG(strlen(path) + 1 + size + sizeof(size) + sizeof(offset), WRITE);
	ENCODE_STRING(path);
	ENCODE_OPAQUE(size, buf);
	ENCODE_VALUE(htobe64(offset));
	return msg;
}

int xmp_write(const char *path, const char *buf, size_t size, off_t offset,
			  struct fuse_file_info *fi) {
	int fd;
	int res;
	(void)fi;

	if (send_op(encode_write(path, buf, size, offset)) < 0)
		;

	printf("Write %.*s @ %lu to %s\n", (int)size, buf, offset, path);

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	fd = open(real_path, O_WRONLY);
	if (fd == -1)
		return -errno;

	res = pwrite(fd, buf, size, offset);
	if (res == -1)
		res = -errno;
	close(fd);

	return res;
}

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

int xmp_fallocate(const char *path, int mode, off_t offset, off_t length,
				  struct fuse_file_info *fi) {
	int fd;
	int res;
	(void)fi;

	if (send_op(encode_fallocate(path, mode, offset, length)) < 0)
		;

	if (mode)
		return -EOPNOTSUPP;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	fd = open(real_path, O_WRONLY);
	if (fd == -1)
		return -errno;

	res = -posix_fallocate(fd, offset, length);

	close(fd);

	return res;
}
#endif

#ifdef HAVE_SETXATTR
op_message encode_setxattr(const char *path, const char *name,
						   const char *value, uint64_t size, int32_t flags) {
	NEW_MSG(strlen(path) + 1 + strlen(name) + 1 + size + sizeof(size) +
				sizeof(flags),
			SETXATTR);
	ENCODE_STRING(path);
	ENCODE_STRING(name);
	ENCODE_OPAQUE(size, value);
	ENCODE_VALUE(htobe32(flags));
	return msg;
}

/* xattr operations are optional and can safely be left unimplemented */
int xmp_setxattr(const char *path, const char *name, const char *value,
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

int xmp_removexattr(const char *path, const char *name) {

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

op_message encode_create(const char *path, uint32_t mode) {
	NEW_MSG(strlen(path) + 1 + sizeof(mode), MKDIR);
	ENCODE_STRING(path);
	ENCODE_VALUE(htobe32(mode));
	return msg;
}

int xmp_create(const char *path, mode_t mode, struct fuse_file_info *fi) {
	int res;
	(void)fi;

	if (send_op(encode_create(path, mode)) < 0)
		;

	res = creat(path, mode);
	if (res == -1)
		return -errno;

	return 0;
}
