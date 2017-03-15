CFLAGS=-D_FILE_OFFSET_BITS=64 -Wall -Iinclude `pkg-config fuse3 --cflags`

DEPS = include/defs.h
OBJ = src/main.o src/misc.o src/read.o src/write.o

%.o: %.c $(DEPS)
	gcc -c -o $@ $< $(CFLAGS)

passthrough: $(OBJ)
	gcc -o $@ $^ `pkg-config fuse3 --libs` -L/usr/local/lib

test: passthrough
	mkdir -p mnt_test || true
	fusermount3 -u mnt_test || true
	./passthrough -f --path=`realpath test_path` mnt_test
