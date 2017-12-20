CFLAGS= -g -D_FILE_OFFSET_BITS=64 -Wall -Iinclude `pkg-config fuse3 --cflags`

DEPS = include/defs.h
_OBJ = main.o read.o write.o
OBJ= $(patsubst %,$(ODIR)/%,$(_OBJ))
ODIR=build/fs

$(ODIR)/%.o: src/%.c $(DEPS)
	clang -c -o $@ $< $(CFLAGS)

dirs:
	mkdir test_src || true
	mkdir test_path || true
	rm -rf test_dst || true
	cp -rf test_path test_dst

ll_passthrough: test/passthrough_fh.c
	gcc `pkg-config fuse3 --cflags --libs` -o $@ $^

test_ll: dirs ll_passthrough
	mkdir test_src || true
	fusermount3 -u -z test_src || true
	./ll_passthrough -f test_src

build/fs/passthrough: $(OBJ)
	clang -o $@ $^ `pkg-config fuse3 --libs` -L/usr/local/lib

clean_fs:
	rm -rf build/fs
	mkdir -p build/fs

test_fs: dirs clean_fs build/fs/passthrough
	fusermount3 -u -z test_src || true
	build/fs/passthrough -o allow_other -s -f --path=`realpath test_path` test_src

build/client/client: dirs client/decode.c client/main.c
	rm -rf build/client || true
	mkdir -p build/client
	gcc -c $(CFLAGS) client/decode.c -o build/client/decode.o
	gcc -c $(CFLAGS) client/main.c -o build/client/main.o
	gcc -o build/client/client build/client/*.o

test_client: dirs build/client/client
	build/client/client `realpath test_dst` 127.0.0.1
