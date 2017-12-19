#ifndef _FSYNCER_DEFS_
#define _FSYNCER_DEFS_

#define FUSE_USE_VERSION 30
#ifdef HAVE_CONFIG_H
#include <config.h>
#endif
#ifdef linux
/* For pread()/pwrite()/utimensat() */
#define _XOPEN_SOURCE 700
#endif
#include <byteswap.h>
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <unistd.h>

#define MAX_PATH_SIZE 4096

struct options {
	const char *real_path;
	int port;
	int show_help;
} options;

enum op_type {
	MKNOD,
	MKDIR,
	UNLINK,
	RMDIR,
	SYMLINK,
	RENAME,
	LINK,
	CHMOD,
	CHOWN,
	TRUNCATE,
	WRITE,
	FALLOCATE,
	SETXATTR,
	REMOVEXATTR
};

struct op_msg {
	enum op_type op_type;
	uint32_t op_length;
	unsigned char data[];
};

typedef struct op_msg *op_message;

#if __BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__
#define htobe64(val) bswap_64(val)
#define htobe32(val) bswap_32(val)
#else
#define htobe64(val) val
#define htobe32(val) val
#endif

#endif
