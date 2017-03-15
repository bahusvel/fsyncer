CFLAGS=-D_FILE_OFFSET_BITS=64 -Wall -Iinclude `pkg-config fuse3 --cflags`

DEPS = include/defs.h
_OBJ = main.o misc.o read.o write.o
OBJ= $(patsubst %,$(ODIR)/%,$(_OBJ))
ODIR=build

$(ODIR)/%.o: src/%.c $(DEPS)
	gcc -c -o $@ $< $(CFLAGS)

build/passthrough: $(OBJ)
	gcc -o $@ $^ `pkg-config fuse3 --libs` -L/usr/local/lib

test: build/passthrough
	mkdir -p mnt_test || true
	fusermount3 -u mnt_test || true
	build/passthrough -f --path=`realpath test_path` mnt_test
