#pragma once

#include <stdint.h>
#include "touch_queue.h"

#ifdef __cplusplus
extern "C" {
#endif

#define TD_TOUCH_MAX_ERROR_RUN 5

typedef enum {
    TD_TOUCH_POLL_IDLE,
    TD_TOUCH_POLL_REPORT,
    TD_TOUCH_POLL_ERROR,
} td_touch_poll_t;

/*
 * The real finger, held between controller reports. A poll that finds nothing new says nothing, so the held
 * sample stands until a poll replaces it.
 */
typedef struct {
    td_touch_sample_t sample;
    uint8_t error_run;
} td_touch_held_t;

/*
 * Folds one controller poll into the held sample. `report` is read only for TD_TOUCH_POLL_REPORT.
 * - A report with a finger replaces the sample.
 * - A report without one lifts the press and keeps the last coordinates, so the release lands where the press
 *   ended. The coordinates in `report` do not matter then.
 * - An idle poll leaves the sample alone and ends a run of failed polls.
 * - TD_TOUCH_MAX_ERROR_RUN failed polls in a row lift a held press, so a dead bus cannot hold a finger down for
 *   good. LVGL takes the lift for a release, and a release clicks the object under the finger.
 */
void td_touch_held_poll(td_touch_held_t *h, td_touch_poll_t poll, td_touch_sample_t report);

#ifdef __cplusplus
}
#endif
