#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "esp_console.h"
#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Builds the first screen. Runs once with the LVGL adapter lock held, before the first frame. */
typedef void (*display_build_fn)(void);

/* Brings up the CH422G, the RGB panel and LVGL, calls `build` to fill the first screen, and waits for the first
 * frame. A NULL `build` leaves LVGL's empty screen.
 * Returns ESP_ERR_TIMEOUT when the first frame did not reach the panel in time. */
esp_err_t display_start(display_build_fn build);

/* True once the LVGL adapter has started, which is what the UI's console commands need. */
bool display_ready(void);

/* Frames flushed to the panel since boot. Any task may read it. */
uint32_t display_frame_count(void);

/* The display's console commands, for td_console_register(). Before display_start() and after it failed, a
 * well-formed command answers "display not ready". */
const esp_console_cmd_t *display_console_commands(size_t *count);

#ifdef __cplusplus
}
#endif
