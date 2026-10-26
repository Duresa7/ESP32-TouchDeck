#pragma once

#include <stdbool.h>
#include <stdint.h>
#include "cJSON.h"
#include "td_console.h"

typedef struct {
    bool online;
    uint32_t frames_rx;
    uint32_t frames_tx;
    uint32_t crc_errors;
    uint32_t dropped;
    int64_t last_rx_ms;
} td_link_state_t;

esp_err_t td_link_start(void);
td_link_state_t td_link_state(void);
void td_link_hello(void);
cJSON *td_link_dispatch(const cJSON *request);
const esp_console_cmd_t *td_link_console_commands(size_t *count);
