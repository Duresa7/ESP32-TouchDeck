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

typedef struct {
    int image[2];
    uint32_t min_delay_ms;
    uint32_t page_tiles;
    uint32_t page_pixels;
} td_link_animation_caps_t;

cJSON *td_link_capabilities(void);

#define TD_LINK_ID_MAX 16

esp_err_t td_link_start(void);

/* Applies the photo frame's stored idle settings to the power gate and the idle face. Call it once after power_start;
 * v2.idle.set applies later changes. */
void td_link_idle_boot(void);

/* Queues a `press` event for a tap on an app or widget tile, from any task and without blocking. It is dropped when
 * the queue is full or the app is not connected. */
void td_link_press(const char *page, const char *tile, const char *kind);
td_link_state_t td_link_state(void);
void td_link_hello(void);
cJSON *td_link_dispatch(const cJSON *request);
const esp_console_cmd_t *td_link_console_commands(size_t *count);
