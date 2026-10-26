#pragma once
#include "key_sink.h"
#define KEY_SINK_FANOUT_MAX 3
typedef struct {
    key_sink_t sinks[KEY_SINK_FANOUT_MAX];
} key_sink_fanout_t;
/* The owner keeps fanout alive for the lifetime of the engine. NULL slots are skipped. */
key_sink_t key_sink_fanout_sink(key_sink_fanout_t *fanout);
