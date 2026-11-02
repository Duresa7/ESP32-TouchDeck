#include "esp_heap_caps.h"
#include <stdbool.h>
#include <stdlib.h>

static size_t live;
static bool fail_next;

static void *track(void *ptr)
{
    if (ptr) live++;
    return ptr;
}

static bool refuse(void)
{
    const bool refused = fail_next;
    fail_next = false;
    return refused;
}

void *heap_caps_malloc(size_t bytes, uint32_t caps)
{
    (void)caps;
    return refuse() ? NULL : track(malloc(bytes));
}

void *heap_caps_calloc(size_t count, size_t bytes, uint32_t caps)
{
    (void)caps;
    return refuse() ? NULL : track(calloc(count, bytes));
}

void heap_caps_free(void *ptr)
{
    if (!ptr) return;
    live--;
    free(ptr);
}

size_t host_heap_live(void) { return live; }
void host_heap_fail_next(void) { fail_next = true; }
