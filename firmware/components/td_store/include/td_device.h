#pragma once
#include <stdbool.h>
#include "esp_err.h"
#include "td_idle.h"
#define TD_DEVICE_NAME_MAX 48
bool td_device_name_valid(const char *name);
esp_err_t td_device_name_get(char out[TD_DEVICE_NAME_MAX + 1]);
esp_err_t td_device_name_set(const char *name);
/* The photo frame's idle settings in NVS namespace td_device, key idle. Missing or unreadable reads as the defaults. */
esp_err_t td_device_idle_get(td_idle_settings_t *out);
/* Commits valid settings before it returns. ESP_ERR_INVALID_ARG out of range. */
esp_err_t td_device_idle_set(const td_idle_settings_t *settings);
