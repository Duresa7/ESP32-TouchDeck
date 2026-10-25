#pragma once

#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    size_t tail;
    bool discarding;
} td_rx_sync_t;

/*
 * Decides what the input task queues for one read of n bytes in buf, given the
 * free bytes in the queue. On return buf holds the m bytes to queue, which is
 * the return value, and *dropped has grown by the bytes that are lost.
 */
size_t td_rx_sync(td_rx_sync_t *s, char *buf, size_t n, size_t space, size_t *dropped);

#ifdef __cplusplus
}
#endif
