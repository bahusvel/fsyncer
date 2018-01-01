#ifndef _FSYNCER_H_
#define _FSYNCER_H_

#define FUSE_USE_VERSION 31
#define HAVE_UTIMENSAT 1

#ifdef HAVE_CONFIG_H
#include <config.h>
#endif

#ifdef linux
/* For pread()/pwrite()/utimensat() */
#define _XOPEN_SOURCE 700
#endif

#include <fuse.h>

#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <unistd.h>

struct options {
	const char *real_path;
	int port;
	int consistent;
	int dontcheck;
	int show_help;
} options;

#endif
