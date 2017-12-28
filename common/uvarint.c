#include "uvarint.h"

// PutUvarint encodes a uint64 into buf and returns the number of bytes written.
// If the buffer is too small, PutUvarint will panic.
int put_uvarint(unsigned char *buf, uint64_t x) {
	int i = 0;
	while (x >= 0x80) {
		buf[i] = ((char)x) | 0x80;
		x >>= 7;
		i++;
	}
	buf[i] = (char)x;
	return i + 1;
}

// Uvarint decodes a uint64 from buf and returns that value and the
// number of bytes read (> 0). If an error occurred, the value is 0
// and the number of bytes n is <= 0 meaning:
//
// 	n == 0: buf too small
// 	n  < 0: value larger than 64 bits (overflow)
// 	        and -n is the number of bytes read
//
int uvarint(unsigned char *buf, size_t buf_size, uint64_t *res) {
	uint64_t x = 0;
	unsigned int s = 0;

	*res = 0;
	for (int i = 0; i < buf_size; i++) {
		unsigned char b = buf[i];
		if (b < 0x80) {
			if (i > 9 || (i == 9 && b > 1)) {

				return -(i + 1); // overflow
			}
			*res = x | ((uint64_t)b) << s;
			return i + 1;
		}
		x |= ((uint64_t)(b & 0x7f)) << s;
		s += 7;
	}
	return 0;
}
