#define _XOPEN_SOURCE 500
#include "fscompare.h"
#include <ftw.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Using ftw from POSIX https://linux.die.net/man/3/ftw

static unsigned long global_hash = 0;
static int root_path_len = 0;

#define hash(x) global_hash = ((global_hash << 5) + global_hash) + x

void string_hash(const char *str) {
	int c;

	while ((c = *str++))
		global_hash =
			((global_hash << 5) + global_hash) + c; /* hash * 33 + c */
}

static int display_info(const char *fpath, const struct stat *sb, int tflag,
						struct FTW *ftwbuf) {
	// clang-format off
    printf("%-3s %7jd %-40s\n",
        (tflag == FTW_D) ?   "d"   : (tflag == FTW_DNR) ? "dnr" :
        (tflag == FTW_DP) ?  "dp"  : (tflag == FTW_F) ?   "f" :
        (tflag == FTW_NS) ?  "ns"  : (tflag == FTW_SL) ?  "sl" :
        (tflag == FTW_SLN) ? "sln" : "???",
        (intmax_t) sb->st_size, fpath+root_path_len);
	// clang-format on
	return 0; /* To tell nftw() to continue */
}

static int hash_ftentry(const char *fpath, const struct stat *sb, int tflag,
						struct FTW *ftwbuf) {
	string_hash(fpath + root_path_len);
	hash(sb->st_size);
	hash(sb->st_mtime);
	return 0;
}

unsigned long hash_metadata(const char *path) {
	global_hash = 5381;
	root_path_len = strlen(path);
	if (nftw(path, hash_ftentry, 20, FTW_PHYS) == -1) {
		perror("nftw");
		exit(EXIT_FAILURE);
	}
	return global_hash;
}

unsigned long hash_data(const char *path) {}
