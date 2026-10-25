#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* LVGL reads the pointer once per period and takes one sample each time, so a swipe lasts its step count
 * times this period. */
#define TD_TOUCH_STEP_MS 10
#define TD_TOUCH_SWIPE_MAX_MS 2000
#define TD_TOUCH_QUEUE_CAP 256

_Static_assert(TD_TOUCH_SWIPE_MAX_MS / TD_TOUCH_STEP_MS + 2 <= TD_TOUCH_QUEUE_CAP,
               "the longest swipe must fit an empty queue");

/* What LVGL's read callback hands over. A release keeps the coordinates of the last press. */
typedef struct {
    uint16_t x;
    uint16_t y;
    bool pressed;
} td_touch_sample_t;

/*
 * Injected samples waiting for LVGL, oldest first. A zeroed queue is empty and ready. Nothing here locks,
 * so the owner serializes the console task that pushes with the LVGL task that pops.
 */
typedef struct {
    td_touch_sample_t items[TD_TOUCH_QUEUE_CAP];
    uint16_t head;
    uint16_t count;
} td_touch_queue_t;

/*
 * Appends all n samples or none, and never waits. False when fewer than n slots are free, so a gesture
 * never loses its release and leaves a press stuck.
 */
bool td_touch_queue_push(td_touch_queue_t *q, const td_touch_sample_t *samples, size_t n);

/* Removes the oldest sample into *out. False when the queue is empty, leaving *out alone. */
bool td_touch_queue_pop(td_touch_queue_t *q, td_touch_sample_t *out);

size_t td_touch_queue_count(const td_touch_queue_t *q);

size_t td_touch_tap(uint16_t x, uint16_t y, td_touch_sample_t out[2]);

/*
 * A swipe presses at (x1, y1), moves in even steps of TD_TOUCH_STEP_MS to (x2, y2), and releases there.
 * Its duration rounds down to whole steps, with at least one. Returns the sample count, which is the step
 * count plus 2 (the press and the release), or 0 when ms is 0 or above TD_TOUCH_SWIPE_MAX_MS or when cap is
 * too small.
 */
size_t td_touch_swipe(uint16_t x1, uint16_t y1, uint16_t x2, uint16_t y2, uint32_t ms, td_touch_sample_t *out,
                      size_t cap);

/*
 * The sample for one LVGL read. An injected sample wins over the real one, pressed or not, and sets *injected.
 * With none queued the real sample passes through and *injected is false. LVGL sees a real press that an
 * injection interrupts as one press that jumps to the injected point.
 */
td_touch_sample_t td_touch_next(td_touch_queue_t *q, td_touch_sample_t real, bool *injected);

#ifdef __cplusplus
}
#endif
