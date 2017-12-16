#include <endian.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

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

#define SIZEOF_MODE_T 4
#define SIZEOF_DEV_T 8
#define SIZEOF_UID_T 4
#define SIZEOF_GID_T 4

struct op_message {
	enum op_type op_type;
	uint32_t op_length;
	unsigned char data[];
};

typedef struct op_message *op_message;

int send_op(op_message message) {}
#define ENCODE_STRING(str)                                                     \
	memcpy(msg_data + i, str, strlen(str) + 1);                                \
	i += strlen(str) + 1;
#define ENCODE_VALUE(val)                                                      \
	*(typeof(val) *)(msg_data + i) = val;                                      \
	i += sizeof(val);
#define ENCODE_OPAQUE(size, buf)                                               \
	ENCODE_VALUE(size);                                                        \
	memcpy(msg_data + i, buf, size);                                           \
	i += size;

#define NEW_MSG(size, type)                                                    \
	size_t tmp_size = (size) + sizeof(struct op_message);                      \
	op_message msg = malloc(tmp_size);                                         \
	msg->op_type = type;                                                       \
	msg->op_length = tmp_size;                                                 \
	off_t i = 0;                                                               \
	unsigned char *msg_data = msg->data;

op_message encode_mknod(const char *path, mode_t mode, dev_t rdev) {
	NEW_MSG(strlen(path) + 1 + SIZEOF_MODE_T + SIZEOF_DEV_T, MKNOD);
	ENCODE_STRING(path);
	ENCODE_VALUE(mode);
	ENCODE_VALUE(rdev);
	return msg;
}
void decode_mknod() {}

op_message encode_mkdir(const char *path, mode_t mode) {
	NEW_MSG(strlen(path) + 1 + SIZEOF_MODE_T, MKDIR);
	ENCODE_STRING(path);
	ENCODE_VALUE(mode);
	return msg;
}
void decode_mkdir() {}

op_message encode_unlink(const char *path) {
	NEW_MSG(strlen(path) + 1, UNLINK);
	ENCODE_STRING(path);
	return msg;
}
void decode_unlink() {}

op_message encode_rmdir(const char *path) {
	NEW_MSG(strlen(path), RMDIR);
	ENCODE_STRING(path);
	return msg;
}
void decode_rmdir() {}

op_message encode_symlink(const char *from, const char *to) {
	NEW_MSG(strlen(from) + 1 + strlen(to) + 1, SYMLINK);
	ENCODE_STRING(from);
	ENCODE_STRING(to);
	return msg;
}
void decode_symlink() {}

op_message encode_rename(const char *from, const char *to, uint32_t flags) {
	NEW_MSG(strlen(from) + 1 + strlen(to) + 1 + sizeof(flags), RENAME);
	ENCODE_STRING(from);
	ENCODE_STRING(to);
	ENCODE_VALUE(flags);
	return msg;
}
void decode_rename() {}

op_message encode_link(const char *from, const char *to) {
	NEW_MSG(strlen(from) + 1 + strlen(to) + 1, LINK);
	ENCODE_STRING(from);
	ENCODE_STRING(to);
	return msg;
}
void decode_link() {}

op_message encode_chmod(const char *path, mode_t mode) {
	NEW_MSG(strlen(path) + 1 + SIZEOF_MODE_T, CHMOD);
	ENCODE_STRING(path);
	ENCODE_VALUE(mode);
	return msg;
}
void decode_chmod() {}

op_message encode_chown(const char *path, uid_t uid, gid_t gid) {
	NEW_MSG(strlen(path) + 1 + SIZEOF_UID_T + SIZEOF_GID_T, CHOWN);
	ENCODE_STRING(path);
	ENCODE_VALUE(uid);
	ENCODE_VALUE(gid);
	return msg;
}
void decode_chown() {}

op_message encode_truncate(const char *path, int64_t size) {
	NEW_MSG(strlen(path) + 1 + sizeof(size), TRUNCATE);
	ENCODE_STRING(path);
	ENCODE_VALUE(size);
	return msg;
}
void decode_truncate() {}

op_message encode_write(const char *path, const char *buf, uint64_t size,
						int64_t offset) {
	NEW_MSG(strlen(path) + 1 + size + sizeof(size) + sizeof(offset), WRITE);
	ENCODE_STRING(path);
	ENCODE_OPAQUE(size, buf);
	ENCODE_VALUE(offset);
	return msg;
}
void decode_write() {}

op_message encode_fallocate(const char *path, int32_t mode, int64_t offset,
							int64_t length) {
	NEW_MSG(strlen(path) + 1 + sizeof(mode) + sizeof(offset) + sizeof(length),
			FALLOCATE);
	ENCODE_STRING(path);
	ENCODE_VALUE(mode);
	ENCODE_VALUE(offset);
	ENCODE_VALUE(length);
	return msg;
}
void decode_fallocate() {}

op_message encode_setxattr(const char *path, const char *name,
						   const char *value, uint64_t size, int32_t flags) {
	NEW_MSG(strlen(path) + 1 + strlen(name) + 1 + size + sizeof(size) +
				sizeof(flags),
			SETXATTR);
	ENCODE_STRING(path);
	ENCODE_STRING(name);
	ENCODE_OPAQUE(size, value);
	ENCODE_VALUE(flags);
	return msg;
}
void decode_setxattr() {}

op_message encode_removexattr(const char *path, const char *name) {
	NEW_MSG(strlen(path) + 1 + strlen(name) + 1, REMOVEXATTR);
	ENCODE_STRING(path);
	ENCODE_STRING(name);
	return msg;
}
void decode_removexattr() {}
