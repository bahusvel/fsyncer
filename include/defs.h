#ifndef _FSYNCER_DEFS_
#define _FSYNCER_DEFS_

#include <stdint.h>
#include <string.h>

#define HAVE_UTIMENSAT 1
#define HAVE_SETXATTR 1
#define HAVE_POSIX_FALLOCATE 1
#define HAVE_FSTATAT 1

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
	OPEN,
	RELEASE
};

enum client_mode { MODE_ASYNC, MODE_SYNC, MODE_CONTROL };
enum command { CMD_CORK, CMD_UNCORK };

struct command_msg {
	enum command cmd;
};

struct init_msg {
	enum client_mode mode;
	uint64_t dsthash;
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

#define MAX_PATH_SIZE 4096

static int fake_root(char *dest, const char *root_path, const char *path) {
	if ((strlen(root_path) + strlen(path)) > MAX_PATH_SIZE) {
		return -1;
	}
	strcpy(dest, root_path);
	strcat(dest, path);
	return 0;
}

#endif
