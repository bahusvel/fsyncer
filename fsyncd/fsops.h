#ifndef _FSOPS_H_
#define _FSOPS_H_

#include "config.h"
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/types.h>
#include <sys/xattr.h>
#include <unistd.h>

#include <sys/xattr.h>

int xmp_mknod(const char *path, mode_t mode, dev_t rdev);
int xmp_mkdir(const char *path, mode_t mode);
int xmp_unlink(const char *path);
int xmp_rmdir(const char *path);
int xmp_symlink(const char *from, const char *to);
int xmp_rename(const char *from, const char *to, unsigned int flags);
int xmp_link(const char *from, const char *to);
int xmp_chmod(const char *path, mode_t mode, int fd);
int xmp_chown(const char *path, uid_t uid, gid_t gid, int fd);
int xmp_truncate(const char *path, off_t size, int fd);
int xmp_write(const char *path, const unsigned char *buf, size_t size, off_t offset,
			  int fd);
#ifdef HAVE_POSIX_FALLOCATE
int xmp_fallocate(const char *path, int mode, off_t offset, off_t length,
				  int fd);
#endif
#ifdef HAVE_SETXATTR
int xmp_setxattr(const char *path, const char *name, const unsigned char *value,
				 size_t size, int flags);
int xmp_removexattr(const char *path, const char *name);
#endif
int xmp_create(const char *path, mode_t mode, int *fd, int flags);
#ifdef HAVE_UTIMENSAT
int xmp_utimens(const char *path, const struct timespec ts[2], int fd);
#endif

#endif
