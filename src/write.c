#define FUSE_USE_VERSION 30

#ifdef HAVE_CONFIG_H
#include <config.h>
#endif

#ifdef linux
/* For pread()/pwrite()/utimensat() */
#define _XOPEN_SOURCE 700
#endif

#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <fuse.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <unistd.h>
#ifdef HAVE_SETXATTR
#include <sys/xattr.h>
#endif

#include "defs.h"
#include "ops.h"

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

int xmp_mkdir(const char *path, mode_t mode) {
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	res = mkdir(real_path, mode);
	if (res == -1)
		return -errno;

	return 0;
}

int xmp_unlink(const char *path) {
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	res = unlink(real_path);
	if (res == -1)
		return -errno;

	return 0;
}

int xmp_rmdir(const char *path) {
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	res = rmdir(real_path);
	if (res == -1)
		return -errno;

	return 0;
}

int xmp_symlink(const char *from, const char *to) {
	int res;

	char real_from[MAX_PATH_SIZE];
	fake_root(real_from, options.real_path, from);

	char real_to[MAX_PATH_SIZE];
	fake_root(real_to, options.real_path, to);

	res = symlink(real_from, real_to);
	if (res == -1)
		return -errno;

	return 0;
}

int xmp_rename(const char *from, const char *to, unsigned int flags) {
	int res;

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

int xmp_link(const char *from, const char *to) {
	int res;

	char real_from[MAX_PATH_SIZE];
	fake_root(real_from, options.real_path, from);

	char real_to[MAX_PATH_SIZE];
	fake_root(real_to, options.real_path, to);

	res = link(real_from, real_to);
	if (res == -1)
		return -errno;

	return 0;
}

int xmp_chmod(const char *path, mode_t mode, struct fuse_file_info *fi) {
	(void)fi;
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	res = chmod(real_path, mode);
	if (res == -1)
		return -errno;

	return 0;
}

int xmp_chown(const char *path, uid_t uid, gid_t gid,
			  struct fuse_file_info *fi) {
	(void)fi;
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	res = lchown(real_path, uid, gid);
	if (res == -1)
		return -errno;

	return 0;
}

int xmp_truncate(const char *path, off_t size, struct fuse_file_info *fi) {
	(void)fi;
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	res = truncate(real_path, size);
	if (res == -1)
		return -errno;

	return 0;
}

int xmp_write(const char *path, const char *buf, size_t size, off_t offset,
			  struct fuse_file_info *fi) {
	int fd;
	int res;
	(void)fi;

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
int xmp_fallocate(const char *path, int mode, off_t offset, off_t length,
				  struct fuse_file_info *fi) {
	int fd;
	int res;
	(void)fi;
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
/* xattr operations are optional and can safely be left unimplemented */
int xmp_setxattr(const char *path, const char *name, const char *value,
				 size_t size, int flags) {

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	int res = lsetxattr(real_path, name, value, size, flags);
	if (res == -1)
		return -errno;
	return 0;
}

int xmp_removexattr(const char *path, const char *name) {

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	int res = lremovexattr(real_path, name);
	if (res == -1)
		return -errno;
	return 0;
}

#endif
