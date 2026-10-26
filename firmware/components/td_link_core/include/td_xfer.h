#pragma once
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "td_frame.h"

#define TD_XFER_WINDOW 4
#define TD_XFER_RETRY_MS 200
#define TD_XFER_TRANSMISSIONS 5
#define TD_XFER_IDLE_MS 2000

typedef bool (*td_xfer_sink_t)(void *ctx, const uint8_t *data, size_t len);
typedef struct {
    uint8_t id;
    bool sending;
    bool aborted;
    uint32_t bytes, crc32, received, crc;
    uint16_t next, ahead;
    uint32_t sent_at[TD_XFER_WINDOW];
    uint8_t transmissions[TD_XFER_WINDOW];
    td_xfer_sink_t sink;
    void *context;
} td_xfer_t;

uint32_t td_xfer_crc32(const uint8_t *data, size_t len);
void td_xfer_init(td_xfer_t *x, uint8_t id, bool sending, uint32_t bytes, uint32_t crc32,
                  td_xfer_sink_t sink, void *context);
/* Only contiguous chunks reach the sink. Duplicates and gaps return the same cumulative ack. */
bool td_xfer_receive(td_xfer_t *x, uint16_t idx, const uint8_t *data, size_t len);
bool td_xfer_complete(const td_xfer_t *x);
void td_xfer_ack(td_xfer_t *x, uint16_t next);
/* Returns a due index, or -1. Five total transmissions, then aborted becomes true. */
int td_xfer_due(td_xfer_t *x, uint32_t now_ms);
