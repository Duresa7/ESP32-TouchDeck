#pragma once

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * How fast the controller reported while one finger stayed down. A contact starts at the first pressed
 * report after a release and ends at the next release. After it ends the figures stay, so a reader can
 * look at them once the finger is lifted. A zeroed struct has seen nothing.
 */
typedef struct {
    int64_t first_us;
    int64_t last_us;
    uint32_t frames;
    bool down;
} td_touch_rate_t;

/* Feeds one fresh controller report with the time it was read. Polls that found no new report do not count. */
void td_touch_rate_frame(td_touch_rate_t *r, bool pressed, int64_t now_us);

/* Milliseconds from the first to the last pressed report of the latest contact. */
uint32_t td_touch_rate_span_ms(const td_touch_rate_t *r);

/* Reports per second across the latest contact, rounded to the nearest. 0 until two reports are at least a
 * microsecond apart. */
uint32_t td_touch_rate_hz(const td_touch_rate_t *r);

#ifdef __cplusplus
}
#endif
