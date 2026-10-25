#pragma once

#include <stddef.h>
#include "esp_console.h"
#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Brings up the CH422G, the RGB panel and LVGL and shows the test screen.
 * Returns ESP_ERR_TIMEOUT when the first frame did not reach the panel in time. */
esp_err_t display_start(void);

/* The screenshot, perf and backlight console commands, for td_console_register(). They stay safe to run
 * before display_start() and after it failed, and answer "display not ready" instead. */
const esp_console_cmd_t *display_console_commands(size_t *count);

#ifdef __cplusplus
}
#endif
