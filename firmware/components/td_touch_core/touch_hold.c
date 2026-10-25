#include "touch_hold.h"

void td_touch_held_poll(td_touch_held_t *h, td_touch_poll_t poll, td_touch_sample_t report)
{
    if (poll != TD_TOUCH_POLL_ERROR) {
        h->error_run = 0;
    } else if (h->error_run < TD_TOUCH_MAX_ERROR_RUN) {
        h->error_run++;
    }

    if (poll == TD_TOUCH_POLL_REPORT) {
        if (report.pressed) {
            h->sample = report;
        } else {
            h->sample.pressed = false;
        }
    } else if (h->error_run >= TD_TOUCH_MAX_ERROR_RUN) {
        h->sample.pressed = false;
    }
}
