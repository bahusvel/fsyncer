passthrough: src/passthrough.c
	gcc -Wall src/passthrough.c `pkg-config fuse3 --cflags --libs` -o passthrough

test: passthrough
	mkdir -p mnt_test || true
	fusermount3 -u mnt_test || true
	./passthrough -f mnt_test
