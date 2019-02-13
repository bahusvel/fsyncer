#define HAVE_UTIMENSAT 1
#define HAVE_SETXATTR 1
#define HAVE_POSIX_FALLOCATE 1
#define HAVE_FSTATAT 1
#define MAX_PATH_SIZE 4096
#define FUSE_USE_VERSION 31

#ifdef linux
/* For pread()/pwrite()/utimensat() */
#define _XOPEN_SOURCE 700
#endif

#include <fuse.h>

#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/types.h>
#include <sys/xattr.h>
#include <unistd.h>

#include <sys/xattr.h>

struct xmp_dirp {
	DIR *dp;
	struct dirent *entry;
	off_t offset;
};

int xmp_readdir(const char *path, void *buf, fuse_fill_dir_t filler,
				off_t offset, struct fuse_file_info *fi) {
	DIR *dp;
	struct dirent *de;

	(void)offset;
	(void)fi;

	dp = opendir(path);
	if (dp == NULL)
		return -errno;

	while ((de = readdir(dp)) != NULL) {
		struct stat st;
		memset(&st, 0, sizeof(st));
		st.st_ino = de->d_ino;
		st.st_mode = de->d_type << 12;
		if (filler(buf, de->d_name, &st, 0))
			break;
	}

	closedir(dp);
	return 0;
}
/*
int xmp_read_buf(const char *path, struct fuse_bufvec **bufp, size_t size,
				 off_t offset, struct fuse_file_info *fi) {
	struct fuse_bufvec *src;

	(void)path;

	src = malloc(sizeof(struct fuse_bufvec));
	if (src == NULL)
		return -ENOMEM;

	*src = FUSE_BUFVEC_INIT(size);

	src->buf[0].flags = FUSE_BUF_IS_FD | FUSE_BUF_FD_SEEK;
	src->buf[0].fd = fi->fh;
	src->buf[0].pos = offset;

	*bufp = src;

	return 0;
}*/