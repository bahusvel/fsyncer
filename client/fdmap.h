#ifndef _FDMAP_H_
#define _FDMAP_H_

typedef void *fdmap_t;

fdmap_t fdmap_new();
void fdmap_free(fdmap_t map);
void fdmap_set(fdmap_t map, int key, int value);
int fdmap_get(fdmap_t map, int key);

#endif
