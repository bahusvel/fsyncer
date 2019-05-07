#include <unistd.h>
#include <fcntl.h>
#include <string.h>
#include <error.h>
#include <stdio.h>
#include <sys/stat.h>
#include <stdlib.h>

const int NUM_CYCLES = 100;
const int FLAGS = O_CREAT | O_WRONLY | O_TRUNC;

int main(int argc, char **argv) {
    char * buf = "hello";
    int fd = open(argv[1], FLAGS, 0775);
    int writes_to_flush = atoi(argv[2]);
    if (!writes_to_flush) {
        printf("Second argument must be number of writes before a flush");
        exit(-1);
    }
    if (fd == -1) {
        perror("open");
    }
    if (fchmod(fd,0775)) {
        perror("chmod");
    }
    for (int j = 0; j < NUM_CYCLES; j++) {
        for (int i = 0; i < writes_to_flush; i++) {
            if (write(fd, buf, strlen(buf)) != strlen(buf)) {
                printf("fd %d\n", fd);
                perror("write");
            }
        }
        if (fsync(fd)) {
            perror("fsync");
        }
    }
    close(fd);
}