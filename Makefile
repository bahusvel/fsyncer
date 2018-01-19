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

test_fs: dirs
	fusermount3 -u -z test_src || true
	cd fsyncd && cargo run -- --server `realpath ../test_path` -- -f

test_client:
	rm -rf test_dst || true
	cp -rax test_path test_dst
	cd client && cargo run -- --client 127.0.0.1:2323 `realpath ../test_dst`
