#include "defs.h"
#include <stdlib.h>

char *dst_path;

#if __BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__
#define htobe64(val) bswap_64(val)
#define be64toh(val) bswap_64(val)
#define htobe32(val) bswap_32(val)
#define be32toh(val) bswap_32(val)
#elif __BYTE_ORDER__ == __ORDER_BIG_ENDIAN__
#define htobe64(val) val
#define be64toh(val) val
#define htobe32(val) val
#define be32toh(val) val
#endif

#define DECODE_STRING()                                                        \
	(const char *)encoded;                                                     \
	encoded += strlen((const char *)encoded) + 1

#define DECODE_VALUE(type, convert)                                            \
	convert(*(type *)encoded);                                                 \
	encoded += sizeof(type)

#define DECODE_OPAQUE_SIZE() (size_t) be32toh(*(uint32_t *)encoded)
#define DECODE_OPAQUE()                                                        \
	(const char *)(encoded + sizeof(uint32_t));                                \
	encoded += be32toh(*(uint32_t *)encoded) + sizeof(uint32_t)

#define DECODE_FIXED_SIZE(size)                                                \
	(encoded);                                                                 \
	encoded += size;

static int fake_root(char *dest, const char *root_path, const char *path) {
	if ((strlen(root_path) + strlen(path)) > MAX_PATH_SIZE) {
		return -1;
	}
	strcpy(dest, root_path);
	strcat(dest, path);
	return 0;
}

int xmp_mknod(const char *path, mode_t mode, dev_t rdev) {
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, dst_path, path);

	/* On Linux this could just be 'mknod(path, mode, rdev)' but this
	   is more portable */
	if (S_ISFIFO(mode))
		res = mkfifo(real_path, mode);
	else
		res = mknod(real_path, mode, rdev);
	if (res == -1)
		return -errno;

	return 0;
}

int do_mknod(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	mode_t mode = DECODE_VALUE(uint32_t, be32toh);
	dev_t rdev = DECODE_VALUE(uint32_t, be32toh);
	return xmp_mknod(path, mode, rdev);
}

int xmp_mkdir(const char *path, mode_t mode) {
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, dst_path, path);

	res = mkdir(real_path, mode);
	if (res == -1)
		return -errno;

	return 0;
}

int do_mkdir(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	mode_t mode = DECODE_VALUE(uint32_t, be32toh);
	return xmp_mkdir(path, mode);
}

int xmp_unlink(const char *path) {
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, dst_path, path);

	res = unlink(real_path);
	if (res == -1)
		return -errno;

	return 0;
}

int do_unlink(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	return xmp_unlink(path);
}

int xmp_rmdir(const char *path) {
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, dst_path, path);

	res = rmdir(real_path);
	if (res == -1)
		return -errno;

	return 0;
}

int do_rmdir(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	return xmp_rmdir(path);
}

int xmp_symlink(const char *from, const char *to) {
	int res;

	char real_from[MAX_PATH_SIZE];
	fake_root(real_from, dst_path, from);

	char real_to[MAX_PATH_SIZE];
	fake_root(real_to, dst_path, to);

	res = symlink(real_from, real_to);
	if (res == -1)
		return -errno;

	return 0;
}

int do_symlink(unsigned char *encoded) {
	const char *from = DECODE_STRING();
	const char *to = DECODE_STRING();
	return xmp_symlink(from, to);
}

int xmp_rename(const char *from, const char *to, unsigned int flags) {
	int res;

	if (flags)
		return -EINVAL;

	char real_from[MAX_PATH_SIZE];
	fake_root(real_from, dst_path, from);

	char real_to[MAX_PATH_SIZE];
	fake_root(real_to, dst_path, to);

	res = rename(real_from, real_to);
	if (res == -1)
		return -errno;

	return 0;
}

int do_rename(unsigned char *encoded) {
	const char *from = DECODE_STRING();
	const char *to = DECODE_STRING();
	unsigned int flags = DECODE_VALUE(uint32_t, be32toh);
	return xmp_rename(from, to, flags);
}

int xmp_link(const char *from, const char *to) {
	int res;

	char real_from[MAX_PATH_SIZE];
	fake_root(real_from, dst_path, from);

	char real_to[MAX_PATH_SIZE];
	fake_root(real_to, dst_path, to);

	res = link(real_from, real_to);
	if (res == -1)
		return -errno;

	return 0;
}

int do_link(unsigned char *encoded) {
	const char *from = DECODE_STRING();
	const char *to = DECODE_STRING();
	return xmp_link(from, to);
}

int xmp_chmod(const char *path, mode_t mode) {
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, dst_path, path);

	res = chmod(real_path, mode);
	if (res == -1)
		return -errno;

	return 0;
}

int do_chmod(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	mode_t mode = DECODE_VALUE(uint32_t, be32toh);
	return xmp_chmod(path, mode);
}

int xmp_chown(const char *path, uid_t uid, gid_t gid) {
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, dst_path, path);

	res = lchown(real_path, uid, gid);
	if (res == -1)
		return -errno;

	return 0;
}

int do_chown(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	uid_t uid = DECODE_VALUE(uint32_t, be32toh);
	gid_t gid = DECODE_VALUE(uint32_t, be32toh);
	return xmp_chown(path, uid, gid);
}

int xmp_truncate(const char *path, off_t size) {
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, dst_path, path);

	res = truncate(real_path, size);
	if (res == -1)
		return -errno;

	return 0;
}

int do_truncate(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	off_t size = DECODE_VALUE(int64_t, be64toh);
	return xmp_truncate(path, size);
}

int xmp_write(const char *path, const char *buf, size_t size, off_t offset) {
	int fd;
	int res;

	// printf("Write %.*s @ %lu to %s\n", (int)size, buf, offset, path);

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, dst_path, path);

	fd = open(real_path, O_WRONLY);
	if (fd == -1)
		return -errno;

	res = pwrite(fd, buf, size, offset);
	if (res == -1)
		res = -errno;

	close(fd);
	return res;
}

int do_write(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	size_t size = DECODE_OPAQUE_SIZE();
	const char *buf = DECODE_OPAQUE();
	off_t offset = DECODE_VALUE(int64_t, be64toh);
	return xmp_write(path, buf, size, offset);
}

#ifdef HAVE_POSIX_FALLOCATE
int xmp_fallocate(const char *path, int mode, off_t offset, off_t length) {
	int fd;
	int res;
	if (mode)
		return -EOPNOTSUPP;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, dst_path, path);

	fd = open(real_path, O_WRONLY);
	if (fd == -1)
		return -errno;

	res = -posix_fallocate(fd, offset, length);

	close(fd);
	return res;
}
int do_fallocate(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	int mode = DECODE_VALUE(int32_t, be32toh);
	off_t offset = DECODE_VALUE(int64_t, be64toh);
	off_t length = DECODE_VALUE(int64_t, be64toh);
	return xmp_fallocate(path, mode, offset, length);
}
#endif

#ifdef HAVE_SETXATTR
/* xattr operations are optional and can safely be left unimplemented */
int xmp_setxattr(const char *path, const char *name, const char *value,
				 size_t size, int flags) {

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, dst_path, path);

	int res = lsetxattr(real_path, name, value, size, flags);
	if (res == -1)
		return -errno;
	return 0;
}

int do_setxattr(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	const char *name = DECODE_STRING();
	size_t size = DECODE_OPAQUE_SIZE();
	const char *value = DECODE_OPAQUE();
	int flags = DECODE_VALUE(int32_t, be32toh);
	return xmp_setxattr(path, name, value, size, flags);
}

int xmp_removexattr(const char *path, const char *name) {

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, dst_path, path);

	int res = lremovexattr(real_path, name);
	if (res == -1)
		return -errno;
	return 0;
}

int do_removexattr(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	const char *name = DECODE_STRING();
	return xmp_removexattr(path, name);
}
#endif

static int xmp_create(const char *path, mode_t mode, int flags) {
	int fd;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, dst_path, path);

	// printf("Create %s %d %d\n", real_path, mode, flags);

	fd = open(real_path, flags, mode);

	if (fd != -1)
		close(fd);
	else
		return -errno;

	return 0;
}

int do_create(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	mode_t mode = DECODE_VALUE(uint32_t, be32toh);
	int flags = DECODE_VALUE(int32_t, be32toh);
	return xmp_create(path, mode, flags);
}

#ifdef HAVE_UTIMENSAT
int xmp_utimens(const char *path, const struct timespec ts[2]) {
	int res;

	/* don't use utime/utimes since they follow symlinks */

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, dst_path, path);
	res = utimensat(0, real_path, ts, AT_SYMLINK_NOFOLLOW);

	if (res == -1)
		return -errno;

	return 0;
}

int do_utimens(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	const struct timespec *ts =
		(const struct timespec *)DECODE_FIXED_SIZE(sizeof(struct timespec) * 2);
	return xmp_utimens(path, ts);
}
#endif

int do_call(op_message message) {
	switch (message->op_type) {
	case MKNOD:
		return do_mknod(message->data);
	case MKDIR:
		return do_mkdir(message->data);
	case UNLINK:
		return do_unlink(message->data);
	case RMDIR:
		return do_rmdir(message->data);
	case SYMLINK:
		return do_symlink(message->data);
	case RENAME:
		return do_rename(message->data);
	case LINK:
		return do_link(message->data);
	case CHMOD:
		return do_chmod(message->data);
	case CHOWN:
		return do_chown(message->data);
	case TRUNCATE:
		return do_truncate(message->data);
	case WRITE:
		return do_write(message->data);
#ifdef HAVE_POSIX_FALLOCATE
	case FALLOCATE:
		return do_fallocate(message->data);
#endif
#ifdef HAVE_SETXATTR
	case SETXATTR:
		return do_setxattr(message->data);
	case REMOVEXATTR:
		return do_removexattr(message->data);
#endif
	case CREATE:
		return do_create(message->data);
#ifdef HAVE_UTIMENSAT
	case UTIMENS:
		return do_utimens(message->data);
#endif
	default: {
		printf("Unknown vfs call!");
		exit(-1);
	}
	}
}
