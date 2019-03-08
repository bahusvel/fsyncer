CFLAGS= -g -D_FILE_OFFSET_BITS=64 -Wall -Iinclude `pkg-config fuse3 --cflags`

ifneq ($(foreground), no)
	FUSE_FLAGS += -f
endif

ifneq ($(backtrace), no)
	ENV += RUST_BACKTRACE=1
endif

ifneq ($(profile),)
	CARGO_BUILD_FLAGS = --release
	SERVER_FLAGS += --flush-interval 0
	ifeq ($(profile), pprof)
	CARGO_BUILD_FLAGS += --features profile
	endif
	ifeq ($(profile), perf)
		FUSE_FLAGS = -o allow_root
		POST_CMD = (sudo perf record -e 'syscalls:sys_enter_writev' -p `pidof fsyncd` || true) && killall fsyncd
	endif
	ifeq ($(profile), callgrind)
		FUSE_FLAGS += -f -o allow_root
		EXEC_CMD = valgrind --tool=callgrind
	endif
endif

ifeq ($(release), no)
	FSYNCD_BIN = target/debug/fsyncd
else 
	CARGO_BUILD_FLAGS += --release
	FSYNCD_BIN = target/release/fsyncd
endif

ifeq ($(journal), bilog)
	SERVER_FLAGS += --journal=bilog 
endif

ifneq ($(journal),) 
	ifneq ($(journal_size),)
		JOURNAL_SIZE = $(journal_size)
	else 
		JOURNAL_SIZE = 1M
	endif
	SERVER_FLAGS += --journal-size $(JOURNAL_SIZE)
endif

ifneq ($(connect),)
	CLIENT_FLAGS += $(connect)
else
	CLIENT_FLAGS += 127.0.0.1
endif

ifneq ($(sync),)
	CLIENT_FLAGS += --sync=$(sync)
else
	CLIENT_FLAGS += --sync=async
endif

ifneq ($(stream),)
	CLIENT_FLAGS ++ --stream-compressor=$(stream)
endif

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

build:
	cd fsyncd && cargo build $(CARGO_BUILD_FLAGS)

fs: build dirs
	fusermount3 -u -z test_src || true
	$(ENV) $(EXEC_CMD) $(FSYNCD_BIN) server $(SERVER_FLAGS) ./test_src -- $(FUSE_FLAGS)
	$(POST_CMD)

winfs: build dirs
	# fusermount3 -u -z test_src || true
	$(ENV) runas /user:Administrator "$(EXEC_CMD) $(FSYNCD_BIN) server $(SERVER_FLAGS) ./test_src -- $(FUSE_FLAGS)"
	$(POST_CMD)

client: build dirs
	rm -rf test_dst || true
	cp -rax .fsyncer-test_src test_dst
	$(ENV) $(EXEC_CMD) $(FSYNCD_BIN) client ./test_dst $(CLIENT_FLAGS)
	$(POST_CMD)

cmd: build dirs
	ifeq ($(command),)
		exit
	endif
	$(FSYNCD_BIN) control localhost $(command)

journal: build
	$(FSYNCD_BIN) journal 

compile_tests:
	gcc test/sync_test.c -o test/sync_test
	gcc test/direct_test.c -o test/direct_test

mirror_windows:
	cmd.exe /C 'call "C:\Program Files (x86)\Microsoft Visual Studio\2017\Community\Common7\Tools\VsDevCmd.bat" -arch=amd64 && cl.exe /I "C:\Program Files\Dokan\Dokan Library-1.2.1\include" /D _UNICODE /D UNICODE doc/mirror.c /link "C:\Program Files\Dokan\Dokan Library-1.2.1\lib\dokan1.lib" user32.lib advapi32.lib'

test_mirror:
	runas /user:Administrator "mirror.exe /o /s /r C:\Users\denis\Documents\Developing\fsyncer\.fsyncer-test_src /l  C:\Users\denis\Documents\Developing\fsyncer\test_src"