CFLAGS= -g -D_FILE_OFFSET_BITS=64 -Wall -Iinclude `pkg-config fuse3 --cflags`
UNAME := $(shell uname)

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
	FSYNCD_FLAGS += --debug
	ifneq ($(UNAME), Linux)
		FSYNCD_BIN = $(shell cygpath -wa target/debug/fsyncd.exe)
	endif
else 
	CARGO_BUILD_FLAGS += --release
	FSYNCD_BIN = target/release/fsyncd
	ifneq ($(UNAME), Linux)
		FSYNCD_BIN = $(shell cygpath -wa target/release/fsyncd.exe)
	endif
endif

ifneq ($(UNAME), Linux)
	EXEC_CMD = runas /savecred /env /user:Administrator "cmd /k
	END_CMD = "
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

ifneq ($(url),)
	SERVER_FLAGS += $(url)
	CLIENT_FLAGS += $(url)
else
	SERVER_FLAGS += tcp://127.0.0.1:2323
	CLIENT_FLAGS += tcp://127.0.0.1:2323
endif

ifneq ($(sync),)
	CLIENT_FLAGS += --sync=$(sync)
else
	CLIENT_FLAGS += --sync=async
endif

ifneq ($(stream),)
	CLIENT_FLAGS += --stream-compressor=$(stream)
endif

ifneq ($(threads),)
	CLIENT_FLAGS += --threads=$(threads)
endif

ifeq ($(UNAME), Linux)
	CLIENT_CP=cp -rax .fsyncer-test_src test_dst
else
	CLIENT_CP=robocopy .fsyncer-test_src test_dst /mir /it || true
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
	$(ENV) $(EXEC_CMD) $(FSYNCD_BIN) $(FSYNCD_FLAGS) server test_src $(SERVER_FLAGS) -- $(FUSE_FLAGS) $(END_CMD)
	$(POST_CMD)

client: build dirs
	#rm -rf test_dst || true
	#mkdir test_dst
	#$(CLIENT_CP)
	$(ENV) $(EXEC_CMD) $(FSYNCD_BIN) $(FSYNCD_FLAGS) client test_dst $(CLIENT_FLAGS) $(END_CMD)
	$(POST_CMD)

test: build fs client

cmd: build dirs
	ifeq ($(command),)
		exit
	endif
	$(FSYNCD_BIN) control $(command)

journal: build
	$(FSYNCD_BIN) journal 

snapshot_merge:
	(rm test.fs || true) && RUST_BACKTRACE=1 cargo run --bin fsyncd --release -- --debug snapshot test.fs merge test.fj 2>out.txt

snapshot_apply:
	(rm -rf snapshot_test/* || true) && RUST_BACKTRACE=1 cargo run --bin fsyncd --release -- --debug snapshot test.fs apply snapshot_test

flush_sync_perf: compile_tests
	for i in `seq 10 10 1000`; do \
		echo -n "$$i," ; \
		tools/net_time.sh enp0s31f6 test/sync_test test_src/sync.txt $$i | awk '{print $$2 "," $$6}' ; \
	done \

compile_tests:
	gcc test/sync_test.c -o test/sync_test
	gcc test/direct_test.c -o test/direct_test

mirror_windows:
	cmd.exe /C 'call "C:\Program Files (x86)\Microsoft Visual Studio\2017\Community\Common7\Tools\VsDevCmd.bat" -arch=amd64 && cl.exe /I "C:\Program Files\Dokan\Dokan Library-1.2.1\include" /D _UNICODE /D UNICODE doc/mirror.c /link "C:\Program Files\Dokan\Dokan Library-1.2.1\lib\dokan1.lib" user32.lib advapi32.lib'

test_mirror:
	runas /savecred /user:Administrator "doc\mirror.exe /o /r C:\Users\denis\Documents\Developing\fsyncer\.fsyncer-test_src /l  C:\Users\denis\Documents\Developing\fsyncer\test_src"