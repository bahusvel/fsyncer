#ifndef _CODEC_H_
#define _CODEC_H_

#include <byteswap.h>
#include <stdint.h>
#include <string.h>

#if __BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__
#define htobe64(val) bswap_64(val)
#define be64toh(val) bswap_64(val)
#define htobe32(val) bswap_32(val)
#define be32toh(val) bswap_32(val)
#elif __BYTE_ORDER__ == __ORDER_BIG_ENDIAN__
#define htobe64(val) val
#define be64toh(val) val
#define htobe32(val) val
#define be32toh(val) val
#endif

#define ENCODE_STRING(str)                                                     \
	memcpy(msg_data, str, strlen(str) + 1);                                    \
	msg_data += strlen(str) + 1;
#define ENCODE_VALUE(val)                                                      \
	*(typeof(val) *)(msg_data) = val;                                          \
	msg_data += sizeof(val);
#define ENCODE_FIXED_SIZE(size, buf)                                           \
	memcpy(msg_data, buf, size);                                               \
	msg_data += size;
#define ENCODE_OPAQUE(size, buf)                                               \
	ENCODE_VALUE(htobe32(size));                                               \
	ENCODE_FIXED_SIZE(size, buf);

#define NEW_MSG(size, type)                                                    \
	size_t tmp_size = (size) + sizeof(struct op_msg);                          \
	op_message msg = malloc(tmp_size);                                         \
	msg->op_type = type;                                                       \
	msg->op_length = tmp_size;                                                 \
	unsigned char *msg_data = msg->data;

#define DECODE_STRING()                                                        \
	(const char *)encoded;                                                     \
	encoded += strlen((const char *)encoded) + 1

#define DECODE_VALUE(type, convert)                                            \
	convert(*(type *)encoded);                                                 \
	encoded += sizeof(type)

#define DECODE_OPAQUE_SIZE() (size_t) be32toh(*(uint32_t *)encoded)
#define DECODE_OPAQUE()                                                        \
	(const char *)(encoded + sizeof(uint32_t));                                \
	encoded += be32toh(*(uint32_t *)encoded) + sizeof(uint32_t)

#define DECODE_FIXED_SIZE(size)                                                \
	(void*)(encoded);                                                          \
	encoded += size;

#endif
