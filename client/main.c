#include "defs.h"
#include <arpa/inet.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/socket.h>

int do_call(op_message message);

char *dst_path;

int main(int argc, char **argv) {
	int port = 2323;

	if (argc != 3 && argc != 4) {
		printf("Usage: fsyncer_client <sync_dst> <server_address> [server "
			   "port]\n");
		exit(-1);
	}

	dst_path = argv[1];
	char *host = argv[2];

	if (argc == 4) {
		port = atoi(argv[3]);
	}

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

	printf("Connected to %s\n", host);

	char rcv_buf[33 * 1024]; // 32k for max_write + 1k for headers
	op_message msg = (op_message)rcv_buf;
	while (1) {
		int received = 0, n = 0;
		while (received < sizeof(struct op_msg)) {
			n = recv(sock, rcv_buf + received, sizeof(struct op_msg) - received,
					 0);
			if (n <= 0) {
				printf("recv failed %d/%lu\n", received, sizeof(struct op_msg));
				exit(-1);
			}
			received += n;
		}
		while (received < msg->op_length) {
			n = recv(sock, rcv_buf + received, msg->op_length - received, 0);
			if (n <= 0) {
				printf("recv failed %d/%d\n", received, msg->op_length);
				exit(-1);
			}
			received += n;
		}
		printf("Received message %d %d\n", msg->op_type, msg->op_length);
		if (do_call(msg) < 0) {
			perror("error in replay");
		}
	}

	return 0;
}
