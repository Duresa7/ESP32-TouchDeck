#pragma once

#include <stdint.h>

#define TD_TEST_BAR_COUNT 8

/* Left to right. The firmware draws these exactly and the verification skill checks the pixels against them. */
extern const uint16_t td_test_bars_rgb565[TD_TEST_BAR_COUNT];

/* Builds the test screen on the active screen. The caller holds the LVGL adapter lock. */
void test_screen_build(void);
