#pragma once

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

/* Renders the active screen into the one PSRAM snapshot buffer and returns a view of it. Only the console
 * task calls it, so the view stays valid until its next call. ESP_ERR_TIMEOUT when LVGL is too busy to lock. */
esp_err_t display_snapshot(td_shot_image_t *out);

/* Invalidates the whole screen every LVGL cycle for `seconds` and counts the frames flushed to the panel.
 * Blocks the caller for that time. */
esp_err_t display_perf_run(uint32_t seconds, display_perf_t *out);

void display_cmds_register(void);
