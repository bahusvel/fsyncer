/*
 * Generic map implementation.
 */
#include "fdmap.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

uint32_t hash(uint32_t a) {
	a = (a + 0x7ed55d16) + (a << 12);
	a = (a ^ 0xc761c23c) ^ (a >> 19);
	a = (a + 0x165667b1) + (a << 5);
	a = (a + 0xd3a2646c) ^ (a << 9);
	a = (a + 0xfd7046c5) + (a << 3);
	a = (a ^ 0xb55a4f09) ^ (a >> 16);
	return a;
}

#define INITIAL_SIZE (256)
#define MAX_CHAIN_LENGTH (8)

/* We need to keep keys and values */
typedef struct _hashmap_element {
	int key;
	int in_use;
	any_t data;
} hashmap_element;

/* A hashmap has some maximum size and current size,
 * as well as the data to hold. */
typedef struct _hashmap_map {
	int table_size;
	int size;
	hashmap_element *data;
} hashmap_map;

/*
 * Return an empty hashmap, or NULL on failure.
 */
map_t hashmap_new() {
	hashmap_map *m = (hashmap_map *)malloc(sizeof(hashmap_map));
	if (!m)
		goto err;

	m->data = (hashmap_element *)calloc(INITIAL_SIZE, sizeof(hashmap_element));
	if (!m->data)
		goto err;

	m->table_size = INITIAL_SIZE;
	m->size = 0;

	return m;
err:
	if (m)
		hashmap_free(m);
	return NULL;
}

/*
 * Hashing function for a string
 */
unsigned int hashmap_hash_int(hashmap_map *m, int key) {
	return hash(key) % m->table_size;
}

/*
 * Return the integer of the location in data
 * to store the point to the item, or MAP_FULL.
 */
int hashmap_hash(map_t in, int key) {
	int curr;
	int i;

	/* Cast the hashmap */
	hashmap_map *m = (hashmap_map *)in;

	/* If full, return immediately */
	if (m->size >= (m->table_size / 2))
		return MAP_FULL;

	/* Find the best index */
	curr = hashmap_hash_int(m, key);

	/* Linear probing */
	for (i = 0; i < MAX_CHAIN_LENGTH; i++) {
		if (m->data[curr].in_use == 0)
			return curr;

		if (m->data[curr].in_use == 1 && (m->data[curr].key == key))
			return curr;

		curr = (curr + 1) % m->table_size;
	}

	return MAP_FULL;
}

/*
 * Doubles the size of the hashmap, and rehashes all the elements
 */
int hashmap_rehash(map_t in) {
	int i;
	int old_size;
	hashmap_element *curr;

	/* Setup the new elements */
	hashmap_map *m = (hashmap_map *)in;
	hashmap_element *temp =
		(hashmap_element *)calloc(2 * m->table_size, sizeof(hashmap_element));
	if (!temp)
		return MAP_OMEM;

	/* Update the array */
	curr = m->data;
	m->data = temp;

	/* Update the size */
	old_size = m->table_size;
	m->table_size = 2 * m->table_size;
	m->size = 0;

	/* Rehash the elements */
	for (i = 0; i < old_size; i++) {
		int status;

		if (curr[i].in_use == 0)
			continue;

		status = hashmap_put(m, curr[i].key, curr[i].data);
		if (status != MAP_OK)
			return status;
	}

	free(curr);

	return MAP_OK;
}

/*
 * Add a pointer to the hashmap with some key
 */
int hashmap_put(map_t in, int key, any_t value) {
	int index;
	hashmap_map *m;

	/* Cast the hashmap */
	m = (hashmap_map *)in;

	/* Find a place to put our value */
	index = hashmap_hash(in, key);
	while (index == MAP_FULL) {
		if (hashmap_rehash(in) == MAP_OMEM) {
			return MAP_OMEM;
		}
		index = hashmap_hash(in, key);
	}

	/* Set the data */
	m->data[index].data = value;
	m->data[index].key = key;
	m->data[index].in_use = 1;
	m->size++;

	return MAP_OK;
}

/*
 * Get your pointer out of the hashmap with a key
 */
int hashmap_get(map_t in, int key, any_t *arg) {
	int curr;
	int i;
	hashmap_map *m;

	/* Cast the hashmap */
	m = (hashmap_map *)in;

	/* Find data location */
	curr = hashmap_hash_int(m, key);

	/* Linear probing, if necessary */
	for (i = 0; i < MAX_CHAIN_LENGTH; i++) {

		int in_use = m->data[curr].in_use;
		if (in_use == 1) {
			if (m->data[curr].key == key) {
				*arg = (m->data[curr].data);
				return MAP_OK;
			}
		}

		curr = (curr + 1) % m->table_size;
	}

	*arg = 0;

	/* Not found */
	return MAP_MISSING;
}

/*
 * Remove an element with that key from the map
 */
int hashmap_remove(map_t in, int key) {
	int i;
	int curr;
	hashmap_map *m;

	/* Cast the hashmap */
	m = (hashmap_map *)in;

	/* Find key */
	curr = hashmap_hash_int(m, key);

	/* Linear probing, if necessary */
	for (i = 0; i < MAX_CHAIN_LENGTH; i++) {

		int in_use = m->data[curr].in_use;
		if (in_use == 1) {
			if (m->data[curr].key == key) {
				/* Blank out the fields */
				m->data[curr].in_use = 0;
				m->data[curr].data = 0;
				m->data[curr].key = 0;

				/* Reduce the size */
				m->size--;
				return MAP_OK;
			}
		}
		curr = (curr + 1) % m->table_size;
	}

	/* Data not found */
	return MAP_MISSING;
}

/* Deallocate the hashmap */
void hashmap_free(map_t in) {
	hashmap_map *m = (hashmap_map *)in;
	free(m->data);
	free(m);
}

/* Return the length of the hashmap */
int hashmap_length(map_t in) {
	hashmap_map *m = (hashmap_map *)in;
	if (m != NULL)
		return m->size;
	else
		return 0;
}
