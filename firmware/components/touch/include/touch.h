#pragma once

#include <stddef.h>
#include "esp_console.h"
#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Starts the GT911 on the board's I2C bus at address 0x5D and registers the LVGL pointer device. Call it after
 * display_start(), which brings up LVGL. Fails when the controller does not answer or LVGL has no display, and
 * the touch commands then keep answering "touch not ready".
 */
esp_err_t touch_start(void);

/* The touch console commands, for td_console_register(). Before touch_start() and after it failed, a well-formed
 * command answers "touch not ready". */
const esp_console_cmd_t *touch_console_commands(size_t *count);

#ifdef __cplusplus
}
#endif
