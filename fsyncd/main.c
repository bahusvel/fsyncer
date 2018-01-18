#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "fsyncer.h"

void *xmp_init(struct fuse_conn_info *conn, struct fuse_config *cfg) {
	(void)conn;
	cfg->use_ino = 1;
	// NOTE this makes path NULL to parameters where fi->fh exists. This is evil
	// for the current case of replication. But in future when this is properly
	// handled it can improve performance.
	// refer to
	// https://libfuse.github.io/doxygen/structfuse__config.html#adc93fd1ac03d7f016d6b0bfab77f3863
	// cfg->nullpath_ok = 1;

	/* Pick up changes from lower filesystem right away. This is
	   also necessary for better hardlink support. When the kernel
	   calls the unlink() handler, it does not know the inode of
	   the to-be-removed entry and can therefore not invalidate
	   the cache of the associated inode - resulting in an
	   incorrect st_nlink value being reported for any remaining
	   hardlinks to this inode. */
	// cfg->entry_timeout = 0;
	// cfg->attr_timeout = 0;
	// cfg->negative_timeout = 0;
	cfg->auto_cache = 1;
	conn->max_write = 32 * 1024;

	return NULL;
}

void gen_read_ops(struct fuse_operations *xmp_oper);

void gen_write_ops(struct fuse_operations *xmp_oper);

/*
static void *control_loop(void *arg) {
	int client_fd = (int)arg;
	struct command_msg cmd;
	struct ack_msg ack = {0};
	while (1) {
		ack.retcode = 0;
		if (recv(client_fd, &cmd, sizeof(cmd), MSG_WAITALL) != sizeof(cmd)) {
			perror("Failed receiving command_msg");
			return NULL;
		}
		switch (cmd.cmd) {
		case CMD_CORK:
			ack.retcode = do_cork();
			break;
		case CMD_UNCORK:
			ack.retcode = do_uncork();
			break;
		default:
			ack.retcode = -1;
			break;
		}

		if (send(client_fd, &ack, sizeof(ack), 0) < 0) {
			perror("Unable to ack");
			return NULL;
		}
	}
}
*/

int fsyncer_fuse_main(int argc, char **argv) {
	struct fuse_operations xmp_oper = {0};
	gen_read_ops(&xmp_oper);
	gen_write_ops(&xmp_oper);
	return fuse_main(argc, argv, &xmp_oper, NULL);
}
