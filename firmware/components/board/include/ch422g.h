#pragma once

#include <stdbool.h>
#include <stdint.h>
#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    /* One byte to one 7-bit address. Production wiring is i2c_master_transmit, where a NACK is ESP_ERR_INVALID_RESPONSE. */
    esp_err_t (*write)(void *ctx, uint8_t i2c_addr, uint8_t value);
    void (*delay_ms)(void *ctx, uint32_t ms);
    /* Drives GPIO4 low as an output so the GT911 latches address 0x5D when TP_RST releases. */
    esp_err_t (*hold_touch_int_low)(void *ctx);
    void *ctx;
} ch422g_platform_t;

/* Runs the power-up sequence. Re-runnable: it replaces the platform and the cached output.
 * ESP_ERR_INVALID_ARG for a NULL platform or any NULL callback, leaving the driver as it was.
 * Any other failure stops at the failing step and leaves the driver not started. */
esp_err_t ch422g_start(const ch422g_platform_t *platform);

/* The setters return ESP_ERR_INVALID_STATE, without writing, until a start has succeeded. */
esp_err_t ch422g_set_backlight(bool on);
esp_err_t ch422g_touch_reset(bool asserted);

/* The byte the chip last acknowledged at the IO output register since the last start began. */
uint8_t ch422g_output(void);

#ifdef __cplusplus
}
#endif
