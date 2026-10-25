#pragma once

#include <stddef.h>
#include "esp_console.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Builds the deck on LVGL's active screen: the pages, their tiles and the page dots. It is the build callback for
 * display_start(), so it runs once with the LVGL adapter lock held, before the first frame. */
void ui_build(void);

/* The ui console commands, for td_console_register(). Before the display has started, a well-formed command
 * answers "display not ready". */
const esp_console_cmd_t *ui_console_commands(size_t *count);

#ifdef __cplusplus
}
#endif
