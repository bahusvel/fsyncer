#include "decode.h"
#include <arpa/inet.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/socket.h>

int main(int argc, char **argv) {
	int port = 2323;

	if (argc != 2 && argc != 3) {
		printf("Usage: fsyncer_client <server_address> [server port]\n");
		exit(-1);
	}

	char *host = argv[2];

	if (argc == 3) {
		port = atoi(argv[2]);
	}

	int sock = socket(AF_INET, SOCK_STREAM, 0);
	if (sock == -1) {
		printf("Could not create socket\n");
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

	char rcv_buf[32 * 1024];
	while (1) {
		if (recv(sock, rcv_buf, sizeof(rcv_buf), 0) < 0) {
			printf("recv failed\n");
			break;
		}
		if (do_call((op_message)rcv_buf) < 0) {
			printf("error in replay\n");
		}
	}

	return 0;
}
