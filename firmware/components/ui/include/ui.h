#pragma once

#include <stddef.h>
#include "esp_console.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Reads /storage/layouts/<orientation>.json for the first build. A missing or invalid file logs
 * `layout: <reason>, using builtin`, and the deck starts on the built-in layout. Call it before display_start(). */
void ui_layout_boot(const char *orientation);

/* Builds the deck on LVGL's active screen from the boot layout: the pages, their tiles and the page dots. It is the
 * build callback for display_start(), so it runs once with the LVGL adapter lock held, before the first frame. A
 * `layout reload` later rebuilds the same objects from another layout. */
void ui_build(void);

/* The ui console commands, for td_console_register(). Before the display has started, a well-formed command
 * answers "display not ready". */
const esp_console_cmd_t *ui_console_commands(size_t *count);

#ifdef __cplusplus
}
#endif
