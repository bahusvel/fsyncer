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
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/time.h>
#include <unistd.h>

#define MAX_PATH_SIZE 4096

struct options {
	const char *real_path;
	int async;
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

struct op_message {
	enum op_type op_type;
	uint32_t op_length;
	unsigned char data[];
};

typedef struct op_message *op_message;

static int fake_root(char *dest, const char *root_path, const char *path) {
	if ((strlen(root_path) + strlen(path)) > MAX_PATH_SIZE) {
		return -1;
	}
	strcpy(dest, root_path);
	strcat(dest, path);
	return 0;
}

#endif
