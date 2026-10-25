#pragma once

#include <stdint.h>

#define TD_TEST_BAR_COUNT 8

/* Left to right. The firmware draws these exactly and the verification skill checks the pixels against them. */
extern const uint16_t td_test_bars_rgb565[TD_TEST_BAR_COUNT];

/* The row where the display recipe reads the eight bar centres. */
#define TD_TEST_BAR_PROBE_Y 300

#define TD_TEST_BUTTON_X 292
#define TD_TEST_BUTTON_Y 440
#define TD_TEST_BUTTON_W 440
#define TD_TEST_BUTTON_H 100

/* Builds the test screen on the active screen. The caller holds the LVGL adapter lock. */
void test_screen_build(void);

/* Clicks on the button since boot. Any task may read it. */
uint32_t test_screen_taps(void);
