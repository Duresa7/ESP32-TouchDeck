#pragma once

#include <stddef.h>
#include <stdint.h>
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

/*
 * Injects a press at (x, y) that lasts `ms` (1 to 2000, in whole 10 ms steps) and returns as soon as the samples
 * are queued, so the caller can read the UI or take a screenshot during the hold. touch swipe waits for LVGL and
 * blocks the console, so it cannot do that. Only the console task calls it. ESP_ERR_INVALID_STATE before touch is
 * ready, ESP_ERR_NOT_ALLOWED while a real finger is down, ESP_ERR_NO_MEM when the queue is full, ESP_ERR_INVALID_ARG
 * for an `ms` out of range. Each failure queues nothing.
 */
esp_err_t touch_hold(uint16_t x, uint16_t y, uint32_t ms);

#ifdef __cplusplus
}
#endif
