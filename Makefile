CFLAGS= -D_FILE_OFFSET_BITS=64 -Wall -Iinclude `pkg-config fuse3 --cflags`

DEPS = include/defs.h
_OBJ = main.o misc.o read.o write.o
OBJ= $(patsubst %,$(ODIR)/%,$(_OBJ))
ODIR=build/fs

$(ODIR)/%.o: src/%.c $(DEPS)
	gcc -c -o $@ $< $(CFLAGS)

dirs:
	rm -rf build || true
	mkdir -p build/fs || true
	mkdir -p build/client || true
	mkdir test_src || true
	mkdir test_path || true
	rm -rf test_dst || true
	cp -rf test_path test_dst

ll_passthrough: test/passthrough.c
	gcc `pkg-config fuse3 --cflags --libs` -o $@ $^

test_ll: dirs ll_passthrough
	mkdir test_src || true
	fusermount3 -u test_src || true
	./ll_passthrough -f test_src

build/fs/passthrough: $(OBJ)
	gcc -o $@ $^ `pkg-config fuse3 --libs` -L/usr/local/lib

test_fs: dirs build/fs/passthrough
	fusermount3 -u test_src || true
	build/fs/passthrough -o allow_other -f --path=`realpath test_path` test_src

build/client/client: dirs client/decode.c client/main.c
	gcc -c $(CFLAGS) client/decode.c -o build/client/decode.o
	gcc -c $(CFLAGS) client/main.c -o build/client/main.o
	gcc -o build/client/client build/client/*.o

test_client: dirs build/client/client
	build/client/client `realpath test_dst` 127.0.0.1
