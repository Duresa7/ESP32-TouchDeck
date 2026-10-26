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
 * frame. A NULL `build` leaves LVGL's empty screen. In portrait the screen is the panel turned 90 degrees clockwise,
 * 600 by 1024, until the next boot.
 * Returns ESP_ERR_TIMEOUT when the first frame did not reach the panel in time. */
esp_err_t display_start(bool portrait, display_build_fn build);

/* The orientation display_start was given, landscape before it ran, and the screen's size in it. */
bool display_portrait(void);
uint16_t display_width(void);
uint16_t display_height(void);

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
