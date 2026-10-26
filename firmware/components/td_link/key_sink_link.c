#include "key_sink_link.h"
#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"

typedef struct {
    uint32_t seq;
    uint32_t t;
    key_report_t report;
} entry_t;

static struct {
    portMUX_TYPE lock;
    entry_t entries[KEY_SINK_LINK_QUEUE];
    size_t head;
    size_t count;
    uint32_t seq;
    uint32_t last_t;
    bool resync;
    uint8_t mods;
    uint8_t keys[KEY_REPORT_KEYS];
    uint16_t usage;
} s = { .lock = portMUX_INITIALIZER_UNLOCKED };

static void report(void *ctx, const key_report_t *value)
{
    (void)ctx;
    const uint32_t t = (uint32_t)(esp_timer_get_time() / 1000);
    portENTER_CRITICAL(&s.lock);
    s.seq++;
    s.last_t = t;
    if (value->type == KEY_REPORT_CON) {
        s.usage = value->con.usage;
    } else {
        s.mods = value->kbd.mods;
        memcpy(s.keys, value->kbd.keys, sizeof s.keys);
    }
    if (!s.resync) {
        if (s.count == KEY_SINK_LINK_QUEUE) {
            s.count = 0;
            s.resync = true;
        } else {
            s.entries[(s.head + s.count++) % KEY_SINK_LINK_QUEUE] = (entry_t){ .seq = s.seq, .t = t, .report = *value };
        }
    }
    portEXIT_CRITICAL(&s.lock);
}

key_sink_t key_sink_link_sink(void)
{
    return (key_sink_t){ .report = report, .ctx = NULL };
}

static size_t append_keys(char *buf, size_t cap, size_t len, const uint8_t *keys)
{
    len += snprintf(buf + len, cap - len, "\"keys\":[");
    bool first = true;
    for (unsigned i = 0; i < KEY_REPORT_KEYS; i++) {
        if (keys[i] != 0) {
            len += snprintf(buf + len, cap - len, first ? "%u" : ",%u", (unsigned)keys[i]);
            first = false;
        }
    }
    return len + snprintf(buf + len, cap - len, "]");
}

size_t key_sink_link_next(char *buf, size_t cap)
{
    entry_t entry = { 0 };
    uint8_t mods = 0;
    uint8_t keys[KEY_REPORT_KEYS];
    uint16_t usage = 0;
    bool resync = false;
    portENTER_CRITICAL(&s.lock);
    if (s.resync) {
        resync = true;
        entry.seq = s.seq;
        entry.t = s.last_t;
        mods = s.mods;
        memcpy(keys, s.keys, sizeof keys);
        usage = s.usage;
        s.count = 0;
        s.resync = false;
    } else if (s.count > 0) {
        entry = s.entries[s.head];
        s.head = (s.head + 1) % KEY_SINK_LINK_QUEUE;
        s.count--;
    } else {
        portEXIT_CRITICAL(&s.lock);
        return 0;
    }
    portEXIT_CRITICAL(&s.lock);

    size_t len;
    if (resync) {
        len = snprintf(buf, cap, "{\"m\":\"key.resync\",\"seq\":%u,\"t\":%u,\"mods\":%u,", (unsigned)entry.seq,
                       (unsigned)entry.t, (unsigned)mods);
        len = append_keys(buf, cap, len, keys);
        len += snprintf(buf + len, cap - len, ",\"usage\":%u}", (unsigned)usage);
    } else if (entry.report.type == KEY_REPORT_CON) {
        len = snprintf(buf, cap, "{\"m\":\"key.report\",\"seq\":%u,\"t\":%u,\"type\":\"con\",\"usage\":%u}",
                       (unsigned)entry.seq, (unsigned)entry.t, (unsigned)entry.report.con.usage);
    } else {
        len = snprintf(buf, cap, "{\"m\":\"key.report\",\"seq\":%u,\"t\":%u,\"type\":\"kbd\",\"mods\":%u,",
                       (unsigned)entry.seq, (unsigned)entry.t, (unsigned)entry.report.kbd.mods);
        len = append_keys(buf, cap, len, entry.report.kbd.keys);
        len += snprintf(buf + len, cap - len, "}");
    }
    return len < cap ? len : 0;
}

void key_sink_link_discard(void)
{
    portENTER_CRITICAL(&s.lock);
    s.count = 0;
    s.resync = false;
    portEXIT_CRITICAL(&s.lock);
}
