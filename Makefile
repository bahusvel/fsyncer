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

test_client: build/client/client
	rm -rf test_dst || true
	cp -rax test_path test_dst
	cd client && cargo run -- -s -d `realpath ../test_dst`
