#include "config.h"
#include <errno.h>
#define __USE_ATFILE 1
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/types.h>
#include <sys/xattr.h>
#include <unistd.h>

#include <sys/xattr.h>

int xmp_mknod(const char *path, mode_t mode, dev_t rdev) {
	int res;

	if (S_ISFIFO(mode))
		res = mkfifo(path, mode);
	else
		res = mknod(path, mode, rdev);
	if (res == -1)
		return -errno;

	return 0;
}

int xmp_mkdir(const char *path, mode_t mode) {
	int res;

	res = mkdir(path, mode);
	if (res == -1)
		return -errno;

	return 0;
}

int xmp_unlink(const char *path) {
	int res;

	res = unlink(path);
	if (res == -1)
		return -errno;

	return 0;
}

int xmp_rmdir(const char *path) {
	int res;

	res = rmdir(path);
	if (res == -1)
		return -errno;

	return 0;
}

int xmp_symlink(const char *from, const char *to) {
	int res;

	res = symlink(from, to);
	if (res == -1)
		return -errno;

	return 0;
}

int xmp_rename(const char *from, const char *to, unsigned int flags) {
	int res;

	/* When we have renameat2() in libc, then we can implement flags */
	if (flags)
		return -EINVAL;

	res = rename(from, to);
	if (res == -1)
		return -errno;

	return 0;
}

int xmp_link(const char *from, const char *to) {
	int res;

	res = link(from, to);
	if (res == -1)
		return -errno;

	return 0;
}

int xmp_chmod(const char *path, mode_t mode, int fd) {
	int res;

	if (path == NULL)
		res = fchmod(fd, mode);
	else {
		res = chmod(path, mode);
	}

	if (res == -1)
		return -errno;

	return 0;
}

int xmp_chown(const char *path, uid_t uid, gid_t gid, int fd) {
	int res;

	if (path == NULL)
		res = fchown(fd, uid, gid);
	else {
		res = lchown(path, uid, gid);
	}

	if (res == -1)
		return -errno;

	return 0;
}

int xmp_truncate(const char *path, off_t size, int fd) {
	int res;

	if (path == NULL)
		res = ftruncate(fd, size);
	else {
		res = truncate(path, size);
	}

	if (res == -1)
		return -errno;

	return 0;
}

int xmp_write(const char *path, const unsigned char *buf, size_t size, off_t offset,
			  int fd) {
	int res;
	int opened = 0;

	if (fd == -1) {
		fd = open(path, O_WRONLY);
		if (fd == -1)
			return -errno;
		opened = 1;
	}

	// printf("Write %.*s @ %lu to %s\n", (int)size, buf, offset, path);

	res = pwrite(fd, buf, size, offset);
	if (res == -1)
		res = -errno;

	if (opened == 1) {
		close(fd);
	}

	return res;
}

#ifdef HAVE_POSIX_FALLOCATE
int xmp_fallocate(const char *path, int mode, off_t offset, off_t length,
				  int fd) {

	int opened = 0;
	
	if (mode)
		return -EOPNOTSUPP;

	if (fd == -1) {
		fd = open(path, O_WRONLY);
		if (fd == -1)
			return -errno;
		opened = 1;
	}

	int res = -posix_fallocate(fd, offset, length);
	
	if (opened == 1) {
		close(fd);
	}

	return res;
}
#endif

#ifdef HAVE_SETXATTR
int xmp_setxattr(const char *path, const char *name, const unsigned char *value,
				 size_t size, int flags) {

	int res = lsetxattr(path, name, value, size, flags);
	if (res == -1)
		return -errno;

	return 0;
}
int xmp_removexattr(const char *path, const char *name) {

	int res = lremovexattr(path, name);
	if (res == -1)
		return -errno;

	return 0;
}
#endif

int xmp_create(const char *path, mode_t mode, int *fd, int flags) {

	// printf("Create %s %d %d\n", real_path, mode, fi->flags);

	*fd = open(path, flags, mode);
	if (*fd == -1)
		return -errno;

	return 0;
}

#ifdef HAVE_UTIMENSAT
int xmp_utimens(const char *path, const struct timespec *ts, int fd) {
	int res;

	/* don't use utime/utimes since they follow symlinks */
	if (path == NULL)
		res = futimens(fd, ts);
	else {
		res = utimensat(0, path, ts, AT_SYMLINK_NOFOLLOW);
	}

	if (res == -1)
		return -errno;

	return 0;
}
#endif
