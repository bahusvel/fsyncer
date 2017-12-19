#ifndef _FSYNCER_ENCODE_
#define _FSYNCER_ENCODE_

#include "defs.h"
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

int send_op(op_message message);

op_message encode_mknod(const char *path, uint32_t mode, uint32_t rdev);
op_message encode_mkdir(const char *path, uint32_t mode);
op_message encode_unlink(const char *path);
op_message encode_rmdir(const char *path);
op_message encode_symlink(const char *from, const char *to);
op_message encode_rename(const char *from, const char *to, uint32_t flags);
op_message encode_link(const char *from, const char *to);
op_message encode_chmod(const char *path, uint32_t mode);
op_message encode_chown(const char *path, uint32_t uid, uint32_t gid);
op_message encode_truncate(const char *path, int64_t size);
op_message encode_write(const char *path, const char *buf, uint64_t size,
						int64_t offset);
op_message encode_fallocate(const char *path, int32_t mode, int64_t offset,
							int64_t length);
op_message encode_setxattr(const char *path, const char *name,
						   const char *value, uint64_t size, int32_t flags);
op_message encode_removexattr(const char *path, const char *name);

#endif
