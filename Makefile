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

clean:
	cd fsyncd && cargo clean

compile_tests:
	gcc test/sync_test.c -o test/sync_test
	gcc test/direct_test.c -o test/direct_test

test_fs: dirs
	fusermount3 -u -z test_src || true
	cd fsyncd && RUST_BACKTRACE=1 cargo run -- server --flush-interval 0 ../test_src -- -f -o allow_root

test_client:
	rm -rf test_dst || true
	cp -rax .fsyncer-test_src test_dst
	cd fsyncd && RUST_BACKTRACE=1 cargo run -- client `realpath ../test_dst` 127.0.0.1 --sync=flush --stream-compressor=lz4

test_cork:
	cd fsyncd && RUST_BACKTRACE=1 cargo run -- control localhost cork