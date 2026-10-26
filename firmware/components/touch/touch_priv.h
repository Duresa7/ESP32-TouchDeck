#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "esp_err.h"
#include "touch_queue.h"

typedef struct {
    uint8_t address;
    uint32_t polls;
    uint32_t frames;
    uint32_t pressed_frames;
    uint32_t errors;
    uint32_t injected;
    uint32_t rejected;
    uint32_t queued;
    td_touch_sample_t last;
    uint32_t contact_frames;
    uint32_t contact_ms;
    uint32_t contact_hz;
    uint32_t watch_samples; /* samples the recovery watcher has taken since boot */
    uint32_t watch_ns_avg;  /* what one of them cost, from the CPU cycle counter */
    uint32_t watch_ns_max;
    bool hold;              /* a contact from touch_hold_begin() is down */
} touch_stats_t;

/* True once the GT911 answered and the LVGL pointer device is registered. */
bool touch_ready(void);

/*
 * Queues the samples for LVGL and waits until LVGL has taken them and run one more read, so the click or
 * gesture they cause has been dispatched. *deliver_us is the time from queueing to LVGL taking the last
 * sample. ESP_ERR_INVALID_STATE while a real finger is down, because LVGL would see that press jump to the
 * injected point. ESP_ERR_NO_MEM when the queue cannot hold all of them. Both leave nothing queued.
 * ESP_ERR_INVALID_ARG for n of 0. ESP_ERR_TIMEOUT when LVGL had not taken them all and run one more read within
 * timeout_ms. The samples it had not taken stay queued.
 */
esp_err_t touch_inject(const td_touch_sample_t *samples, size_t n, uint32_t timeout_ms, uint32_t *deliver_us);

void touch_get_stats(touch_stats_t *out);

void touch_set_log(bool on);
