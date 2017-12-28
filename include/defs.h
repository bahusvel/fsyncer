#ifndef _FSYNCER_DEFS_
#define _FSYNCER_DEFS_

#define FUSE_USE_VERSION 31
#define HAVE_UTIMENSAT 1

#ifdef HAVE_CONFIG_H
#include <config.h>
#endif

#ifdef linux
/* For pread()/pwrite()/utimensat() */
#define _XOPEN_SOURCE 700
#endif

#include <fuse.h>

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
	int consistent;
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
	REMOVEXATTR,
	CREATE,
	UTIMENS,
};

enum client_mode { MODE_ASYNC, MODE_SYNC, MODE_CONTROL };
enum command { CMD_CORK, CMD_UNCORK };

struct command_msg {
	enum command cmd;
};

struct init_msg {
	enum client_mode mode;
};

struct ack_msg {
	int retcode;
};

struct op_msg {
	uint32_t op_length;
	enum op_type op_type;
	unsigned char data[];
};

typedef struct op_msg *op_message;

#endif
