#include "td_xfer.h"
#include <string.h>

static uint32_t crc_update(uint32_t crc, const uint8_t *data, size_t len)
{
    for (size_t i = 0; i < len; i++) {
        crc ^= data[i];
        for (unsigned bit = 0; bit < 8; bit++) {
            crc = (crc >> 1) ^ ((crc & 1) ? 0xedb88320U : 0);
        }
    }
    return crc;
}

uint32_t td_xfer_crc32(const uint8_t *data, size_t len)
{
    return ~crc_update(UINT32_MAX, data, len);
}

void td_xfer_init(td_xfer_t *x, uint8_t id, bool sending, uint32_t bytes, uint32_t crc32,
                  td_xfer_sink_t sink, void *context)
{
    *x = (td_xfer_t){ .id = id, .sending = sending, .bytes = bytes, .crc32 = crc32,
                      .crc = UINT32_MAX, .sink = sink, .context = context };
}

bool td_xfer_receive(td_xfer_t *x, uint16_t idx, const uint8_t *data, size_t len)
{
    if (x->sending || x->aborted || len > TD_FRAME_CHUNK_DATA_MAX) {
        return false;
    }
    if (idx < x->next) {
        return true;
    }
    if (idx != x->next || x->received >= x->bytes) {
        return false;
    }
    size_t wanted = x->bytes - x->received;
    if (wanted > TD_FRAME_CHUNK_DATA_MAX) {
        wanted = TD_FRAME_CHUNK_DATA_MAX;
    }
    if (len != wanted || !x->sink || !x->sink(x->context, data, len)) {
        return false;
    }
    x->crc = crc_update(x->crc, data, len);
    x->received += len;
    x->next++;
    return true;
}

bool td_xfer_complete(const td_xfer_t *x)
{
    return !x->aborted && (x->sending
        ? (uint32_t)x->next * TD_FRAME_CHUNK_DATA_MAX >= x->bytes
        : x->received == x->bytes && ~x->crc == x->crc32);
}

void td_xfer_ack(td_xfer_t *x, uint16_t next)
{
    if (x->sending && next > x->next && next <= x->ahead) {
        while (x->next < next) {
            x->transmissions[x->next++ % TD_XFER_WINDOW] = 0;
        }
    }
}

int td_xfer_due(td_xfer_t *x, uint32_t now_ms)
{
    if (!x->sending || x->aborted) {
        return -1;
    }
    for (uint16_t idx = x->next; idx < x->ahead; idx++) {
        unsigned slot = idx % TD_XFER_WINDOW;
        if (now_ms - x->sent_at[slot] >= TD_XFER_RETRY_MS) {
            if (x->transmissions[slot] >= TD_XFER_TRANSMISSIONS) {
                x->aborted = true;
                return -1;
            }
            x->transmissions[slot]++;
            x->sent_at[slot] = now_ms;
            return idx;
        }
    }
    if (x->ahead < x->next + TD_XFER_WINDOW && (uint32_t)x->ahead * TD_FRAME_CHUNK_DATA_MAX < x->bytes) {
        unsigned slot = x->ahead % TD_XFER_WINDOW;
        x->transmissions[slot] = 1;
        x->sent_at[slot] = now_ms;
        return x->ahead++;
    }
    return -1;
}
