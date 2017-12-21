#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

unsigned long hash_metadata(const char *path);

int main(int argc, char **argv) {
	if (argc != 2) {
		printf("Usage: fscompare <path>\n");
		exit(-1);
	}
	if (chdir(argv[1]) < 0) {
		perror("cannot access directory");
		exit(-1);
	}
	printf("%16lx\n", hash_metadata("./"));
	return 0;
}
