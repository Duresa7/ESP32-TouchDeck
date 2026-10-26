#pragma once

#include <stddef.h>
#include "key_sink.h"

#ifdef __cplusplus
extern "C" {
#endif

#define KEY_SINK_LINK_QUEUE 64
#define KEY_SINK_LINK_EVENT_MAX 160

/*
 * The sink that mirrors every report to the app. report() never blocks and never calls back into keys_*, so it is safe
 * under the engine's lock. Each report gets seq, counted from 1 since boot like the log sink's, and t in milliseconds
 * since boot. The 64-slot queue never loses a release: when it is full, the queued reports are thrown away and the
 * sink sends one `key.resync` with the last state it saw, ahead of anything newer.
 */
key_sink_t key_sink_link_sink(void);

/* Writes the JSON of the oldest event to buf and removes it. Returns its length, or 0 when the queue is empty. */
size_t key_sink_link_next(char *buf, size_t cap);

/* Throws away what is queued without sending it, and keeps counting. Used while no app is connected. */
void key_sink_link_discard(void);

#ifdef __cplusplus
}
#endif
