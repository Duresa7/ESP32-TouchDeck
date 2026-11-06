#include "td_input.h"
#include <math.h>
#include <stdio.h>
#include <string.h>

static bool id_valid(const char *s)
{
    if (!s || !*s) return false;
    size_t n = 0;
    for (; s[n]; n++) {
        if (n >= 15 || !((s[n] >= 'a' && s[n] <= 'z') || (s[n] >= 'A' && s[n] <= 'Z') ||
                        (s[n] >= '0' && s[n] <= '9') || s[n] == '_' || s[n] == '-')) return false;
    }
    return true;
}

bool td_input_put(td_input_mailbox_t *box, const char *page, const char *tile, double value, bool end, uint32_t now)
{
    if (!id_valid(page) || !id_valid(tile) || !isfinite(value) || value < 0 || value > 1) return false;
    if (box->pending && box->event.end) return false;
    strcpy(box->event.page, page);
    strcpy(box->event.tile, tile);
    box->event.value = round(value * 10000) / 10000;
    box->event.seq = ++box->seq;
    box->event.t = now;
    box->event.end = end;
    box->pending = true;
    return true;
}

bool td_input_take(td_input_mailbox_t *box, uint32_t now, td_input_event_t *out)
{
    if (!box->pending || (!box->event.end && box->sent && (uint32_t)(now - box->sent_ms) < 50)) return false;
    *out = box->event;
    box->pending = false;
    box->sent = true;
    box->sent_ms = now;
    return true;
}

void td_input_cancel(td_input_mailbox_t *box)
{
    box->pending = false;
    box->sent = false;
}

bool td_press_init(td_press_t *out, const char *page, const char *tile, const char *kind, const char *part,
                   bool v2_selected, uint32_t generation, uint32_t now)
{
    if (!out || !page || !tile || !kind) return false;
    *out = (td_press_t){ .t = now, .generation = generation };
    snprintf(out->page, sizeof out->page, "%s", page);
    snprintf(out->tile, sizeof out->tile, "%s", tile);
    snprintf(out->kind, sizeof out->kind, "%s", kind);
    if (part) {
        if (!v2_selected || !*part || strlen(part) >= sizeof out->part) return false;
        for (const char *c = part; *c; c++) {
            if ((*c < 'a' || *c > 'z') && *c != '_') return false;
        }
        strcpy(out->part, part);
    }
    return true;
}

bool td_press_ready(const td_press_t *press, bool online, bool v2_selected, uint32_t generation)
{
    return press && online && press->generation == generation && (!press->part[0] || v2_selected);
}

size_t td_press_json(const td_press_t *press, char *out, size_t cap)
{
    int len = snprintf(out, cap, "{\"m\":\"press\",\"page\":\"%s\",\"tile\":\"%s\",\"kind\":\"%s\",\"seq\":%u,\"t\":%u}",
                       press->page, press->tile, press->kind, (unsigned)press->seq, (unsigned)press->t);
    if (press->part[0] && len > 0 && (size_t)len < cap) {
        int tail = snprintf(out + len - 1, cap - (size_t)len + 1, ",\"part\":\"%s\"}", press->part);
        len = tail < 0 ? -1 : len - 1 + tail;
    }
    return len > 0 && (size_t)len < cap ? (size_t)len : 0;
}
