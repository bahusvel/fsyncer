CFLAGS= -g -D_FILE_OFFSET_BITS=64 -Wall -Iinclude `pkg-config fuse3 --cflags`

dirs:
	mkdir test_src || true
	mkdir test_path || true

ll_passthrough: test/passthrough_fh.c
	gcc `pkg-config fuse3 --cflags --libs` -o $@ $^

test_ll: dirs ll_passthrough
	mkdir test_src || true
	fusermount3 -u -z test_src || true
	./ll_passthrough -f test_src

test_fs: dirs
	fusermount3 -u -z test_src || true
	cd fs && cargo run -- -o allow_other -f --path=`realpath ../test_path` ../test_src

build/common: common/fscompare.c common/uvarint.c
	rm -rf build/common || true
	mkdir -p build/common
	gcc -c common/fscompare.c -o build/common/fscompare.o $(CLFAGS) -Iinclude
	gcc -c common/uvarint.c -o build/common/uvarint.o $(CLFAGS) -Iinclude

fscompare:
	rm -rf build/fscompare || true
	mkdir -p build/fscompare
	gcc common/fscompare_main.c common/fscompare.c -o build/fscompare/fscompare

build/client/client: dirs build/common client/decode.c client/main.c client/fdmap.rs
	rm -rf build/client || true
	mkdir -p build/client
	gcc -c $(CFLAGS) client/decode.c -o build/client/decode.o
	gcc -c $(CFLAGS) client/main.c -o build/client/main.o
	rustc --crate-type staticlib client/fdmap.rs --out-dir build/client
	gcc -o build/client/client build/client/*.o build/common/*.o -L build/client -lfdmap -ldl -lpthread

test_client: build/client/client
	rm -rf test_dst || true
	cp -rax test_path test_dst
	build/client/client -d `realpath test_dst` -h 127.0.0.1
