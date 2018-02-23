#ifndef _FSYNCER_DEFS_
#define _FSYNCER_DEFS_

#include "config.h"
#include <stdint.h>
#include <string.h>

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
struct op_msg {
	uint32_t op_length;
	enum op_type op_type;
	uint64_t tid;
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
