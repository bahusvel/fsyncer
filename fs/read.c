#include "defs.h"
#include "fsyncer.h"
#include <stdlib.h>

static int xmp_getattr(const char *path, struct stat *stbuf,
					   struct fuse_file_info *fi) {
	int res;

	if (fi)
		res = fstat(fi->fh, stbuf);
	else {
		char real_path[MAX_PATH_SIZE];
		fake_root(real_path, dst_path, path);
		res = lstat(real_path, stbuf);
	}
	if (res == -1)
		return -errno;

	return 0;
}

static int xmp_access(const char *path, int mask) {
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, dst_path, path);

	res = access(real_path, mask);
	if (res == -1)
		return -errno;

	return 0;
}

static int xmp_readlink(const char *path, char *buf, size_t size) {
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, dst_path, path);

	res = readlink(real_path, buf, size - 1);
	if (res == -1)
		return -errno;

	buf[res] = '\0';
	return 0;
}

struct xmp_dirp {
	DIR *dp;
	struct dirent *entry;
	off_t offset;
};

static int xmp_opendir(const char *path, struct fuse_file_info *fi) {
	int res;
	struct xmp_dirp *d = malloc(sizeof(struct xmp_dirp));
	if (d == NULL)
		return -ENOMEM;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, dst_path, path);

	d->dp = opendir(real_path);
	if (d->dp == NULL) {
		res = -errno;
		free(d);
		return res;
	}
	d->offset = 0;
	d->entry = NULL;

	fi->fh = (unsigned long)d;
	return 0;
}

static inline struct xmp_dirp *get_dirp(struct fuse_file_info *fi) {
	return (struct xmp_dirp *)(uintptr_t)fi->fh;
}

static int xmp_readdir(const char *path, void *buf, fuse_fill_dir_t filler,
					   off_t offset, struct fuse_file_info *fi,
					   enum fuse_readdir_flags flags) {
	struct xmp_dirp *d = get_dirp(fi);

	(void)path;
	if (offset != d->offset) {
#ifndef __FreeBSD__
		seekdir(d->dp, offset);
#else
		/* Subtract the one that we add when calling
		   telldir() below */
		seekdir(d->dp, offset - 1);
#endif
		d->entry = NULL;
		d->offset = offset;
	}
	while (1) {
		struct stat st;
		off_t nextoff;
		enum fuse_fill_dir_flags fill_flags = 0;

		if (!d->entry) {
			d->entry = readdir(d->dp);
			if (!d->entry)
				break;
		}
#ifdef HAVE_FSTATAT
		if (flags & FUSE_READDIR_PLUS) {
			int res;

			res = fstatat(dirfd(d->dp), d->entry->d_name, &st,
						  AT_SYMLINK_NOFOLLOW);
			if (res != -1)
				fill_flags |= FUSE_FILL_DIR_PLUS;
		}
#endif
		if (!(fill_flags & FUSE_FILL_DIR_PLUS)) {
			memset(&st, 0, sizeof(st));
			st.st_ino = d->entry->d_ino;
			st.st_mode = d->entry->d_type << 12;
		}
		nextoff = telldir(d->dp);
#ifdef __FreeBSD__
		/* Under FreeBSD, telldir() may return 0 the first time
		   it is called. But for libfuse, an offset of zero
		   means that offsets are not supported, so we shift
		   everything by one. */
		nextoff++;
#endif
		if (filler(buf, d->entry->d_name, &st, nextoff, fill_flags))
			break;

		d->entry = NULL;
		d->offset = nextoff;
	}

	return 0;
}

static int xmp_releasedir(const char *path, struct fuse_file_info *fi) {
	struct xmp_dirp *d = get_dirp(fi);
	(void)path;
	closedir(d->dp);
	free(d);
	return 0;
}

static int xmp_read(const char *path, char *buf, size_t size, off_t offset,
					struct fuse_file_info *fi) {
	int res;

	(void)path;

	// printf("Read %lu\n", fi->fh);

	res = pread(fi->fh, buf, size, offset);
	if (res == -1)
		res = -errno;

	return res;
}

static int xmp_read_buf(const char *path, struct fuse_bufvec **bufp,
						size_t size, off_t offset, struct fuse_file_info *fi) {
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
}

static int xmp_statfs(const char *path, struct statvfs *stbuf) {
	int res;

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, dst_path, path);

	res = statvfs(real_path, stbuf);
	if (res == -1)
		return -errno;

	return 0;
}

static int xmp_flush(const char *path, struct fuse_file_info *fi) {
	int res;

	(void)path;
	/* This is called from every close on an open file, so call the
	   close on the underlying filesystem.	But since flush may be
	   called multiple times for an open file, this must not really
	   close the file.  This is important if used on a network
	   filesystem like NFS which flush the data/metadata on close() */
	res = close(dup(fi->fh));
	if (res == -1)
		return -errno;

	return 0;
}

static int xmp_fsync(const char *path, int isdatasync,
					 struct fuse_file_info *fi) {
	int res;
	(void)path;

#ifndef HAVE_FDATASYNC
	(void)isdatasync;
#else
	if (isdatasync)
		res = fdatasync(fi->fh);
	else
#endif
	res = fsync(fi->fh);
	if (res == -1)
		return -errno;

	return 0;
}

#ifdef HAVE_SETXATTR
int xmp_getxattr(const char *path, const char *name, char *value, size_t size) {

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, dst_path, path);

	int res = lgetxattr(real_path, name, value, size);
	if (res == -1)
		return -errno;
	return res;
}

int xmp_listxattr(const char *path, char *list, size_t size) {

	char real_path[MAX_PATH_SIZE];
	fake_root(real_path, dst_path, path);

	int res = llistxattr(real_path, list, size);
	if (res == -1)
		return -errno;
	return res;
}
#endif

void *xmp_init(struct fuse_conn_info *conn, struct fuse_config *cfg);

void gen_read_ops(struct fuse_operations *xmp_oper) {
	xmp_oper->init = xmp_init;
	xmp_oper->getattr = xmp_getattr;
	xmp_oper->access = xmp_access;
	xmp_oper->readlink = xmp_readlink;
	xmp_oper->opendir = xmp_opendir;
	xmp_oper->readdir = xmp_readdir;
	xmp_oper->releasedir = xmp_releasedir;
	xmp_oper->read = xmp_read;
	xmp_oper->read_buf = xmp_read_buf;
	xmp_oper->statfs = xmp_statfs;
	xmp_oper->flush = xmp_flush;
	xmp_oper->fsync = xmp_fsync;
#ifdef HAVE_SETXATTR
	xmp_oper->getxattr = xmp_getxattr;
	xmp_oper->listxattr = xmp_listxattr;
#endif
}
