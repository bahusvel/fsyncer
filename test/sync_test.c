#include <unistd.h>
#include <fcntl.h>
#include <string.h>
#include <error.h>
#include <stdio.h>

int main(int argc, char **argv) {
    char * buf = "hello";
    int fd = open(argv[1], O_CREAT | O_WRONLY, 0775);
    if (fd == -1) {
        perror("open");
    }
    if (write(fd, buf, strlen(buf)) != strlen(buf)) {
        printf("fd %d\n", fd);
        perror("write");
    }
    if (fsync(fd) == -1) {
        perror("fsync");
    }
    close(fd);
}