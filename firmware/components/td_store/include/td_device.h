#pragma once
#include <stdbool.h>
#include "esp_err.h"
#define TD_DEVICE_NAME_MAX 48
bool td_device_name_valid(const char *name);
esp_err_t td_device_name_get(char out[TD_DEVICE_NAME_MAX + 1]);
esp_err_t td_device_name_set(const char *name);
