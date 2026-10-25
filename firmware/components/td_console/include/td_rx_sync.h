#pragma once

#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Queued after the bytes of a line that a drop cut, so the reader abandons its partial line. */
#define TD_RX_ABANDON '\0'

typedef struct {
    size_t tail;
    bool discarding;
} td_rx_sync_t;

/*
 * Decides what the input task queues for one read of n bytes in buf, given the
 * free bytes in the queue. On return buf holds the m bytes to queue, which is
 * the return value, and *dropped has grown by the host bytes that are lost.
 *
 * A read is queued whole when one byte stays free for TD_RX_ABANDON. Otherwise
 * it is dropped, and so is every byte through the next CR or LF, so no line
 * that spans a drop reaches the reader. The part of that line queued earlier is
 * counted as dropped and cancelled by TD_RX_ABANDON.
 *
 * A NUL from the host is queued as '?', because NUL is the abandon marker.
 */
size_t td_rx_sync(td_rx_sync_t *s, char *buf, size_t n, size_t space, size_t *dropped);

#ifdef __cplusplus
}
#endif
