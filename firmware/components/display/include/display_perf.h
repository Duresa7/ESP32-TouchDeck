#pragma once

/* The perf tools' hooks, only in test builds with CONFIG_TD_PERF_TOOLS (see display/Kconfig). */

#include <stdbool.h>
#include <stdint.h>
#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Queues one tap at (x, y) the way `touch tap` does and returns once LVGL has taken both samples. */
typedef esp_err_t (*display_perf_tap_fn)(uint16_t x, uint16_t y, uint32_t *deliver_us);

/* The touch component hands over its injector, so `perf tap` can tap without the display depending on touch. */
void display_perf_set_tap(display_perf_tap_fn tap);

/* Runs on the LVGL task as LVGL takes an injected sample, so `perf tap` can time a redraw from it. */
void display_perf_input(bool pressed);

/* Records the free and minimum free internal RAM after a boot stage, for `perf mem boot`. `stage` must stay valid
 * until the next reset, so pass a string literal. Only app_main's task calls it, the first time at its start. */
void display_perf_boot_mark(const char *stage);

#ifdef __cplusplus
}
#endif
