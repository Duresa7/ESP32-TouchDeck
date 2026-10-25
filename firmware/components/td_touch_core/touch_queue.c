#include "touch_queue.h"

static size_t ring_index(const td_touch_queue_t *q, size_t offset)
{
    return (q->head + offset) % TD_TOUCH_QUEUE_CAP;
}

static uint16_t along(uint16_t from, uint16_t to, int32_t step, int32_t steps)
{
    const int32_t span = (int32_t)to - (int32_t)from;
    return (uint16_t)(from + span * step / steps);
}

bool td_touch_queue_push(td_touch_queue_t *q, const td_touch_sample_t *samples, size_t n)
{
    const size_t free_slots = TD_TOUCH_QUEUE_CAP - q->count;
    if (n > free_slots) {
        return false;
    }
    for (size_t i = 0; i < n; i++) {
        q->items[ring_index(q, q->count + i)] = samples[i];
    }
    q->count = (uint16_t)(q->count + n);
    return true;
}

bool td_touch_queue_pop(td_touch_queue_t *q, td_touch_sample_t *out)
{
    if (q->count == 0) {
        return false;
    }
    *out = q->items[q->head];
    q->head = (uint16_t)ring_index(q, 1);
    q->count--;
    return true;
}

size_t td_touch_queue_count(const td_touch_queue_t *q)
{
    return q->count;
}

size_t td_touch_tap(uint16_t x, uint16_t y, td_touch_sample_t out[2])
{
    out[0] = (td_touch_sample_t){ .x = x, .y = y, .pressed = true };
    out[1] = (td_touch_sample_t){ .x = x, .y = y, .pressed = false };
    return 2;
}

size_t td_touch_swipe(uint16_t x1, uint16_t y1, uint16_t x2, uint16_t y2, uint32_t ms, td_touch_sample_t *out,
                      size_t cap)
{
    if (ms == 0 || ms > TD_TOUCH_SWIPE_MAX_MS) {
        return 0;
    }
    size_t steps = ms / TD_TOUCH_STEP_MS;
    if (steps == 0) {
        steps = 1;
    }
    const size_t total = steps + 2;
    if (cap < total) {
        return 0;
    }

    out[0] = (td_touch_sample_t){ .x = x1, .y = y1, .pressed = true };
    for (size_t i = 1; i <= steps; i++) {
        out[i] = (td_touch_sample_t){ .x = along(x1, x2, (int32_t)i, (int32_t)steps),
                                      .y = along(y1, y2, (int32_t)i, (int32_t)steps),
                                      .pressed = true };
    }
    out[steps + 1] = (td_touch_sample_t){ .x = x2, .y = y2, .pressed = false };
    return total;
}

td_touch_sample_t td_touch_next(td_touch_queue_t *q, td_touch_sample_t real, bool *injected)
{
    td_touch_sample_t s = real;
    *injected = td_touch_queue_pop(q, &s);
    return s;
}
