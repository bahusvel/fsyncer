#include <unistd.h>
#include <fcntl.h>
#include <string.h>

int main() {
    char * buf = "hello";
    int fd = open("hello.txt", O_CREAT, 0775);
    write(fd,buf, strlen(buf));
    fsync(fd);
}