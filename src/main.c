#ifdef HAVE_CONFIG_H
#include <config.h>
#endif

#ifdef linux
/* For pread()/pwrite()/utimensat() */
#define _XOPEN_SOURCE 700
#endif

#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <fuse.h>
#include <netinet/in.h>
#include <pthread.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/types.h>
#include <unistd.h>
#ifdef HAVE_SETXATTR
#include <sys/xattr.h>
#endif
#include "defs.h"
#include "ops.h"

#define on_error(...)                                                          \
	{                                                                          \
		fprintf(stderr, __VA_ARGS__);                                          \
		fflush(stderr);                                                        \
		exit(1);                                                               \
	}
#define BUFFER_SIZE 1024

static int server_fd = 0;
static int client_fd = 0;

void *xmp_init(struct fuse_conn_info *conn, struct fuse_config *cfg) {
	(void)conn;
	// cfg->use_ino = 1;
	return NULL;
}

static struct fuse_operations xmp_oper = {
	.init = xmp_init,
	.getattr = xmp_getattr,
	.access = xmp_access,
	.readlink = xmp_readlink,
	.readdir = xmp_readdir,
	.symlink = xmp_symlink,
	.unlink = xmp_unlink,
	.rmdir = xmp_rmdir,
	.rename = xmp_rename,
	.link = xmp_link,
	.chmod = xmp_chmod,
	.chown = xmp_chown,
	.truncate = xmp_truncate,
#ifdef HAVE_UTIMENSAT
	.utimens = xmp_utimens,
#endif
	.open = xmp_open,
	.read = xmp_read,
	.write = xmp_write,
	.statfs = xmp_statfs,
	.release = xmp_release,
	.fsync = xmp_fsync,
#ifdef HAVE_POSIX_FALLOCATE
	.fallocate = xmp_fallocate,
#endif
#ifdef HAVE_SETXATTR
	.setxattr = xmp_setxattr,
	.getxattr = xmp_getxattr,
	.listxattr = xmp_listxattr,
	.removexattr = xmp_removexattr,
#endif
};

/*
 * Command line optionsvoid *(*__start_routine)(void *)or the char* fields here
 * because fuse_opt_parse would attempt to free() them when the user specifies
 * different values on the command line.
 */

static void show_help(const char *progname) {
	printf("usage: %s [options] <mountpoint>\n\n", progname);
}

static void *server_loop(void *arg) {
	struct sockaddr_in client;
	int client_fd;
	char buf[BUFFER_SIZE];

	while (1) {
		socklen_t client_len = sizeof(client);
		client_fd = accept(server_fd, (struct sockaddr *)&client, &client_len);
		if (client_fd < 0)
			on_error("Could not establish new connection\n");
		// TODO negotiate with client
		// TODO add client to replicate_to list
	}
}

int send_op(op_message message) {
	if (client_fd == 0) {
		return 0;
	}
	if (send(client_fd, (const void *)message, message->op_length, 0) < 0) {
		perror("Failed sending op to client");
		return -1;
	}
	free(message);
	return 0;
}

#define OPTION(t, p)                                                           \
	{ t, offsetof(struct options, p), 1 }
static const struct fuse_opt option_spec[] = {
	OPTION("--path=%s", real_path), OPTION("--port=%d", port),
	OPTION("-h", show_help), OPTION("--help", show_help), FUSE_OPT_END};

int main(int argc, char *argv[]) {
	umask(0);
	struct fuse_args args = FUSE_ARGS_INIT(argc, argv);

	/* Set defaults -- we have to use strdup so that
	   fuse_opt_parse can free the defaults if other
	   values are specified */
	options.real_path = strdup("/");
	options.port = 2323;

	/* Parse options */
	if (fuse_opt_parse(&args, &options, option_spec, NULL) == -1)
		return 1;

	/* When --help is specified, first print our own file-system
	   specific help text, then signal fuse_main to show
	   additional help (by adding `--help` to the options again)
	   without usage: line (by setting argv[0] to the empty
	   string) */
	if (options.show_help) {
		show_help(argv[0]);
		args.argv[0] = (char *)"";
	}

	server_fd = socket(AF_INET, SOCK_STREAM, 0);

	if (server_fd < 0)
		on_error("Could not create socket\n");

	struct sockaddr_in server = {.sin_family = AF_INET,
								 .sin_port = htons(options.port),
								 .sin_addr.s_addr = htonl(INADDR_ANY)};

	setsockopt(server_fd, SOL_SOCKET, SO_REUSEADDR, 1, sizeof(1));

	int err = bind(server_fd, (struct sockaddr *)&server, sizeof(server));
	if (err < 0)
		on_error("Could not bind socket\n");

	err = listen(server_fd, 128);
	if (err < 0)
		on_error("Could not listen\n");

	pthread_t server_thread;

	err = pthread_create(&server_thread, NULL, server_loop, NULL);
	if (err)
		on_error("Failed to start server thread");

	return fuse_main(args.argc, args.argv, &xmp_oper, NULL);
}
