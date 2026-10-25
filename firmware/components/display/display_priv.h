#pragma once

#include <stdbool.h>
#include <stdint.h>
#include "esp_err.h"
#include "esp_lcd_types.h"
#include "td_shot.h"

typedef struct {
    uint32_t seconds;
    uint32_t frames;
    uint32_t elapsed_ms;
} display_perf_t;

esp_err_t panel_rgb_create(size_t num_fbs, esp_lcd_panel_handle_t *out);

/* Renders the active screen on the LVGL task into the one PSRAM snapshot buffer and returns a view of it. Only
 * the console task calls it, so the view stays valid until its next call. ESP_ERR_INVALID_STATE while a timed-out
 * request's job has not run yet, ESP_ERR_TIMEOUT when LVGL is too busy to lock or to finish in time. */
esp_err_t display_snapshot(td_shot_image_t *out);

/* Invalidates the whole screen every LVGL cycle for `seconds` and counts the frames flushed to the panel.
 * Blocks the caller for that time. */
esp_err_t display_perf_run(uint32_t seconds, display_perf_t *out);

/* True once the LVGL adapter has started, which is what snapshots and perf runs need. */
bool display_ready(void);
