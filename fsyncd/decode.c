#include "codec.h"
#include "defs.h"
#include "fsops.h"

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <unistd.h>

extern char *client_path;

static int do_mknod(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	mode_t mode = DECODE_VALUE(uint32_t, be32toh);
	dev_t rdev = DECODE_VALUE(uint32_t, be32toh);

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, client_path, path);

	return xmp_mknod(real_path, mode, rdev);
}

static int do_mkdir(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	mode_t mode = DECODE_VALUE(uint32_t, be32toh);

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, client_path, path);

	return xmp_mkdir(real_path, mode);
}

static int do_unlink(unsigned char *encoded) {
	const char *path = DECODE_STRING();

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, client_path, path);

	return xmp_unlink(real_path);
}

static int do_rmdir(unsigned char *encoded) {
	const char *path = DECODE_STRING();

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, client_path, path);

	return xmp_rmdir(real_path);
}

static int do_symlink(unsigned char *encoded) {
	const char *from = DECODE_STRING();
	const char *to = DECODE_STRING();

	char real_to[MAX_PATH_SIZE];
	fake_root(real_to, client_path, to);

	return xmp_symlink(from, real_to);
}

static int do_rename(unsigned char *encoded) {
	const char *from = DECODE_STRING();
	const char *to = DECODE_STRING();
	unsigned int flags = DECODE_VALUE(uint32_t, be32toh);

	char real_from[MAX_PATH_SIZE];
	fake_root(real_from, client_path, from);

	char real_to[MAX_PATH_SIZE];
	fake_root(real_to, client_path, to);

	return xmp_rename(real_from, real_to, flags);
}

static int do_link(unsigned char *encoded) {
	const char *from = DECODE_STRING();
	const char *to = DECODE_STRING();

	char real_from[MAX_PATH_SIZE];
	fake_root(real_from, client_path, from);

	char real_to[MAX_PATH_SIZE];
	fake_root(real_to, client_path, to);

	return xmp_link(real_from, real_to);
}

static int do_chmod(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	mode_t mode = DECODE_VALUE(uint32_t, be32toh);

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, client_path, path);

	return xmp_chmod(real_path, mode, -1);
}

static int do_chown(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	uid_t uid = DECODE_VALUE(uint32_t, be32toh);
	gid_t gid = DECODE_VALUE(uint32_t, be32toh);

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, client_path, path);

	return xmp_chown(real_path, uid, gid, -1);
}

static int do_truncate(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	off_t size = DECODE_VALUE(int64_t, be64toh);

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, client_path, path);

	return xmp_truncate(real_path, size, -1);
}

static int dec_write(const char *path, const char *buf, size_t size,
					 off_t offset) {
	int fd;
	int res;

	// printf("Write %.*s @ %lu to %s\n", (int)size, buf, offset, path);

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, client_path, path);

	fd = open(real_path, O_WRONLY);
	if (fd == -1)
		return -errno;

	res = pwrite(fd, buf, size, offset);
	if (res == -1)
		res = -errno;

	close(fd);
	return res;
}

static int do_write(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	off_t offset = DECODE_VALUE(int64_t, be64toh);
	size_t size = DECODE_OPAQUE_SIZE();
	const char *buf = DECODE_OPAQUE();
	return dec_write(path, buf, size, offset);
}

#ifdef HAVE_POSIX_FALLOCATE
static int dec_fallocate(const char *path, int mode, off_t offset,
						 off_t length) {
	int fd;
	int res;
	if (mode)
		return -EOPNOTSUPP;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, client_path, path);

	fd = open(real_path, O_WRONLY);
	if (fd == -1)
		return -errno;

	res = -posix_fallocate(fd, offset, length);

	close(fd);
	return res;
}
static int do_fallocate(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	int mode = DECODE_VALUE(int32_t, be32toh);
	off_t offset = DECODE_VALUE(int64_t, be64toh);
	off_t length = DECODE_VALUE(int64_t, be64toh);
	return dec_fallocate(path, mode, offset, length);
}
#endif

#ifdef HAVE_SETXATTR
/* xattr operations are optional and can safely be left unimplemented */

static int do_setxattr(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	const char *name = DECODE_STRING();
	size_t size = DECODE_OPAQUE_SIZE();
	const char *value = DECODE_OPAQUE();
	int flags = DECODE_VALUE(int32_t, be32toh);

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, client_path, path);

	return xmp_setxattr(real_path, name, value, size, flags);
}

static int do_removexattr(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	const char *name = DECODE_STRING();

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, client_path, path);

	return xmp_removexattr(real_path, name);
}
#endif

static int do_create(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	mode_t mode = DECODE_VALUE(uint32_t, be32toh);
	int flags = DECODE_VALUE(int32_t, be32toh);

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, client_path, path);

	int fd = 0;
	int res = xmp_create(real_path, mode, &fd, flags);

	// Instead of this insert into fdmap
	if (fd != -1)
		close(fd);
	return res;
}

#ifdef HAVE_UTIMENSAT
int do_utimens(unsigned char *encoded) {
	const char *path = DECODE_STRING();
	const struct timespec *ts =
		(const struct timespec *)DECODE_FIXED_SIZE(sizeof(struct timespec) * 2);

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, client_path, path);

	return xmp_utimens(real_path, ts, -1);
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
	case NOP:
	return 0;
	default: {
		printf("Unknown vfs call %d!\n", message->op_type);
		exit(-1);
	}
	}
}
