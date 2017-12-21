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
struct client_entry {
	int fd;
	struct client_entry *next;
};

// FIXME A lock is needed here
static struct client_entry client_list = {0};

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
		int client_fd =
			accept(server_fd, (struct sockaddr *)&client, &client_len);
		if (client_fd < 0)
			on_error("Could not establish new connection\n");
		if (setsockopt(client_fd, SOL_SOCKET, SO_SNDBUF, &(int){1024 * 1024},
					   sizeof(int)) < 0) {
			perror("Failed setting rcvbuf size");
			exit(-1);
		}

		struct client_entry *entry = malloc(sizeof(struct client_entry));
		if (entry == NULL) {
			perror("malloc");
			exit(-1);
		}
		entry->fd = client_fd;
		entry->next = client_list.next;
		client_list.next = entry;

		printf("Client connected!\n");
		// TODO negotiate with client
	}
}

int send_op(op_message message) {
	int res = 0;
	struct client_entry *prev = &client_list;
	struct client_entry *i;
	for (i = client_list.next; i != NULL; prev = i, i = i->next) {
		if (send(i->fd, (const void *)message, message->op_length, 0) < 0) {
			perror("Failed sending op to client");
			prev->next = i->next;
			close(i->fd);
			free(i);
		}
		printf("Sent message %d %d\n", message->op_type, message->op_length);
	}
	free(message);
	return res;
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
