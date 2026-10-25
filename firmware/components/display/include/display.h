#pragma once

#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Brings up the CH422G, the RGB panel and LVGL, shows the test screen and registers the
 * screenshot, perf and backlight console commands. Call it after td_console_start().
 * Returns ESP_ERR_TIMEOUT when the first frame did not reach the panel in time. */
esp_err_t display_start(void);

#ifdef __cplusplus
}
#endif
