#pragma once

#include "td_net.h"

esp_err_t td_net_scan(cJSON *reply);
bool td_net_utf8_valid(const char *s);
