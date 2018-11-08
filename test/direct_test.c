#define _GNU_SOURCE
#include <unistd.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <fcntl.h>
#include <stdio.h>

int main(int argc, char **argv) {
    char * buf = "hello";
    int fd = open(argv[1], O_CREAT | O_WRONLY | O_SYNC, 0775);
    if (fd == -1) {
        perror("open");
    }
    if (write(fd, buf, strlen(buf)) != strlen(buf)) {
        perror("write");
    }
    close(fd);
}