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

static int fake_root(char *dest, const char *root_path, const char *path) {
	if ((strlen(root_path) + strlen(path)) > MAX_PATH_SIZE) {
		return -1;
	}
	strcpy(dest, root_path);
	strcat(dest, path);
	return 0;
}

int xmp_getattr(const char *path, struct stat *stbuf,
				struct fuse_file_info *fi) {
	(void)fi;
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	res = lstat(real_path, stbuf);
	if (res == -1)
		return -errno;

	return 0;
}

int xmp_access(const char *path, int mask) {
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	res = access(real_path, mask);
	if (res == -1)
		return -errno;

	return 0;
}

int xmp_readlink(const char *path, char *buf, size_t size) {
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	res = readlink(real_path, buf, size - 1);
	if (res == -1)
		return -errno;

	buf[res] = '\0';
	return 0;
}

int xmp_readdir(const char *path, void *buf, fuse_fill_dir_t filler,
				off_t offset, struct fuse_file_info *fi,
				enum fuse_readdir_flags flags) {
	DIR *dp;
	struct dirent *de;

	(void)offset;
	(void)fi;
	(void)flags;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	dp = opendir(real_path);
	if (dp == NULL)
		return -errno;

	while ((de = readdir(dp)) != NULL) {
		struct stat st;
		memset(&st, 0, sizeof(st));
		st.st_ino = de->d_ino;
		st.st_mode = de->d_type << 12;
		if (filler(buf, de->d_name, &st, 0, 0))
			break;
	}

	closedir(dp);
	return 0;
}

#ifdef HAVE_UTIMENSAT
int xmp_utimens(const char *path, const struct timespec ts[2],
				struct fuse_file_info *fi) {
	(void)fi;
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	/* don't use utime/utimes since they follow symlinks */
	res = utimensat(0, real_path, ts, AT_SYMLINK_NOFOLLOW);
	if (res == -1)
		return -errno;

	return 0;
}
#endif

int xmp_open(const char *path, struct fuse_file_info *fi) {
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	res = open(real_path, fi->flags);
	if (res == -1)
		return -errno;

	close(res);
	return 0;
}

int xmp_read(const char *path, char *buf, size_t size, off_t offset,
			 struct fuse_file_info *fi) {
	int fd;
	int res;
	(void)fi;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	fd = open(real_path, O_RDONLY);
	if (fd == -1)
		return -errno;

	res = pread(fd, buf, size, offset);
	if (res == -1)
		res = -errno;

	close(fd);
	return res;
}

int xmp_statfs(const char *path, struct statvfs *stbuf) {
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	res = statvfs(real_path, stbuf);
	if (res == -1)
		return -errno;

	return 0;
}

#ifdef HAVE_SETXATTR
int xmp_getxattr(const char *path, const char *name, char *value, size_t size) {

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	int res = lgetxattr(real_path, name, value, size);
	if (res == -1)
		return -errno;
	return res;
}

int xmp_listxattr(const char *path, char *list, size_t size) {

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, options.real_path, path);

	int res = llistxattr(real_path, list, size);
	if (res == -1)
		return -errno;
	return res;
}
#endif
