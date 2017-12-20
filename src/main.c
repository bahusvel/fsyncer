#include <netinet/in.h>
#include <pthread.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>

#include "defs.h"

#define on_error(...)                                                          \
	{                                                                          \
		fprintf(stderr, __VA_ARGS__);                                          \
		fflush(stderr);                                                        \
		exit(1);                                                               \
	}

static int server_fd = 0;
static int client_fd = 0;

void *xmp_init(struct fuse_conn_info *conn, struct fuse_config *cfg) {
	(void)conn;
	cfg->use_ino = 1;
	cfg->nullpath_ok = 1;

	/* Pick up changes from lower filesystem right away. This is
	   also necessary for better hardlink support. When the kernel
	   calls the unlink() handler, it does not know the inode of
	   the to-be-removed entry and can therefore not invalidate
	   the cache of the associated inode - resulting in an
	   incorrect st_nlink value being reported for any remaining
	   hardlinks to this inode. */
	cfg->entry_timeout = 0;
	cfg->attr_timeout = 0;
	cfg->negative_timeout = 0;
	conn->max_write = 32 * 1024;

	return NULL;
}

void gen_read_ops(struct fuse_operations *xmp_oper);

void gen_write_ops(struct fuse_operations *xmp_oper);

static void show_help(const char *progname) {
	printf("usage: %s [options] <mountpoint>\n\n", progname);
}

static void *server_loop(void *arg) {
	struct sockaddr_in client;

	while (1) {
		socklen_t client_len = sizeof(client);
		client_fd = accept(server_fd, (struct sockaddr *)&client, &client_len);
		if (client_fd < 0)
			on_error("Could not establish new connection\n");
		printf("Client connected!\n");
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
	printf("Sent message %d %d\n", message->op_type, message->op_length);

	free(message);
	return 0;
}

#define OPTION(t, p)                                                           \
	{ t, offsetof(struct options, p), 1 }
static const struct fuse_opt option_spec[] = {
	OPTION("--path=%s", real_path),		OPTION("--port=%d", port),
	OPTION("--consistent", consistent), OPTION("-h", show_help),
	OPTION("--help", show_help),		FUSE_OPT_END};

int main(int argc, char *argv[]) {
	umask(0);
	struct fuse_args args = FUSE_ARGS_INIT(argc, argv);

	/* Set defaults -- we have to use strdup so that
	   fuse_opt_parse can free the defaults if other
	   values are specified */
	options.real_path = strdup("/");
	options.port = 2323;
	options.consistent = 1;

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

	setsockopt(server_fd, SOL_SOCKET, SO_REUSEADDR, &(int){1}, sizeof(1));

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

	struct fuse_operations xmp_oper = {0};
	gen_read_ops(&xmp_oper);
	gen_write_ops(&xmp_oper);

	return fuse_main(args.argc, args.argv, &xmp_oper, NULL);
}
