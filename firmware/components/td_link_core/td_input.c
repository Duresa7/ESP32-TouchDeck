#include "td_input.h"
#include <math.h>
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
