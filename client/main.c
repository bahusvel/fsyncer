#include "defs.h"
#include "fscompare.h"
#include <arpa/inet.h>
#include <ctype.h>
#include <netinet/tcp.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/socket.h>

int do_call(op_message message);

char *dst_path;
static int mode_sync = 0;
static int port = 2323;
static char *host = NULL;

int client_connect(unsigned long dsthash) {
	int sock = socket(AF_INET, SOCK_STREAM, 0);
	if (sock == -1) {
		printf("Could not create socket\n");
		exit(-1);
	}

	if (setsockopt(sock, SOL_SOCKET, SO_RCVBUF, &(int){1024 * 1024},
				   sizeof(int)) < 0) {
		perror("Failed setting rcvbuf size");
		exit(-1);
	}

	struct sockaddr_in server = {.sin_family = AF_INET,
								 .sin_port = htons(port),
								 .sin_addr = {.s_addr = inet_addr(host)}};

	if (connect(sock, (struct sockaddr *)&server, sizeof(server)) < 0) {
		perror("connect failed. Error");
		exit(-1);
	}

	if (mode_sync && setsockopt(sock, IPPROTO_TCP, TCP_NODELAY, &(int){1},
								sizeof(int)) < 0) {
		perror("Failed setting nodelay");
		exit(-1);
	}

	printf("Connected to %s\n", host);

	struct init_msg init = {.mode = mode_sync ? MODE_SYNC : MODE_ASYNC,
							.dsthash = dsthash};
	if (send(sock, &init, sizeof(init), 0) < 0) {
		perror("failed sending init");
		exit(-1);
	}

	return sock;
}

void main_loop(int sock) {
	char rcv_buf[33 * 1024]; // 32k for max_write + 1k for headers
	op_message msg = (op_message)rcv_buf;
	while (1) {
		int received = 0, n = 0;
		while (received < sizeof(struct op_msg)) {
			n = recv(sock, rcv_buf + received, sizeof(struct op_msg) - received,
					 MSG_WAITALL);
			if (n <= 0) {
				printf("recv failed %d/%lu\n", received, sizeof(struct op_msg));
				exit(-1);
			}
			received += n;
		}
		while (received < msg->op_length) {
			n = recv(sock, rcv_buf + received, msg->op_length - received,
					 MSG_WAITALL);
			if (n <= 0) {
				printf("recv failed %d/%d\n", received, msg->op_length);
				exit(-1);
			}
			received += n;
		}
		// printf("Received message %d %d\n", msg->op_type, msg->op_length);
		int result = do_call(msg);
		if (result < 0) {
			perror("error in replay");
		}
		if (mode_sync) {
			struct ack_msg ack = {result};
			if (send(sock, &ack, sizeof(ack), 0) < 0) {
				perror("Unable to ack");
				exit(-1);
			}
		}
	}
}

int main(int argc, char **argv) {
	int c;

	while ((c = getopt(argc, argv, "sh:p:d:")) != -1)
		switch (c) {
		case 's':
			mode_sync = 1;
			break;
		case 'h':
			host = optarg;
			break;
		case 'd':
			dst_path = optarg;
			break;
		case 'p':
			port = atoi(optarg);
			break;
		case '?':
			if (optopt == 'p' || optopt == 'd' || optopt == 'h')
				fprintf(stderr, "Option -%c requires an argument.\n", optopt);
			else if (isprint(optopt))
				fprintf(stderr, "Unknown option `-%c'.\n", optopt);
			else
				fprintf(stderr, "Unknown option character `\\x%x'.\n", optopt);
			return 1;
		default:
			abort();
		}

	if (dst_path == NULL || host == NULL) {
		fprintf(stderr, "Both -d and -h must be specified\n");
		exit(-1);
	}

	printf("Calculating destination hash...\n");
	unsigned long dsthash = hash_metadata(dst_path);
	printf("Destinaton hash is %16lx\n", dsthash);

	int sock = client_connect(dsthash);

	main_loop(sock);

	return 0;
}
