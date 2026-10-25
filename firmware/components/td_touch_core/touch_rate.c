#include "touch_rate.h"

void td_touch_rate_frame(td_touch_rate_t *r, bool pressed, int64_t now_us)
{
    if (!pressed) {
        r->down = false;
        return;
    }
    if (!r->down) {
        *r = (td_touch_rate_t){ .first_us = now_us, .last_us = now_us, .frames = 1, .down = true };
        return;
    }
    r->last_us = now_us;
    r->frames++;
}

uint32_t td_touch_rate_span_ms(const td_touch_rate_t *r)
{
    return (uint32_t)((r->last_us - r->first_us) / 1000);
}

uint32_t td_touch_rate_hz(const td_touch_rate_t *r)
{
    const int64_t span_us = r->last_us - r->first_us;
    if (r->frames < 2 || span_us <= 0) {
        return 0;
    }
    const int64_t intervals = r->frames - 1;
    return (uint32_t)((intervals * 1000000 + span_us / 2) / span_us);
}
