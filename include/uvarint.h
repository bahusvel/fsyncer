#ifndef _UVARINT_H_
#define _UVARINT_H_
#include <stddef.h>
#include <stdint.h>

int uvarint(unsigned char *buf, size_t buf_size, uint64_t *res);
int put_uvarint(unsigned char *buf, uint64_t x);

#endif
