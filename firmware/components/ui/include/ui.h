#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "esp_console.h"
#include "td_layout.h"
#include "ui_widget_state.h"

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

#define UI_LAYOUT_ERROR_MAX 196

/* The layout on screen. `error` is the latest refusal, of the boot file or of a candidate a reload kept off the
 * screen, and is empty after a load that worked. */
typedef struct {
    bool from_storage;
    char name[TD_LAYOUT_TITLE_MAX];
    char orientation[12];
    uint8_t pages;
    uint16_t tiles;
    uint16_t icons_loaded;
    uint16_t icons_missing;
    uint32_t bytes;
    uint32_t load_ms;
    char error[UI_LAYOUT_ERROR_MAX];
} ui_layout_info_t;

/* Copies what `layout state` reports, under the layout lock. */
void ui_layout_info(ui_layout_info_t *out);

/* Loads the stored layout for the boot orientation, or the built-in one with `use_builtin`, and swaps it onto the
 * screen. A candidate that fails leaves the current layout on screen and its error in `layout state`. Takes the
 * LVGL lock, so never call it from an LVGL callback. On failure *err says why. */
bool ui_layout_reload(bool use_builtin, td_layout_error_t *err);

/* True once ui_build() has drawn a deck. */
bool ui_ready(void);

/* The orientation the deck booted in, which names the layout file a put replaces and a reload reads. */
const char *ui_layout_slot(void);

/* Called on the LVGL task for every tap on an app or widget tile: the page id, the tile id, and the kind, which is
 * "launch", "open", "command" or "widget". The hook must not block. */
typedef void (*ui_press_hook_t)(const char *page, const char *tile, const char *kind);
void ui_set_press_hook(ui_press_hook_t hook);

/* Tells the deck how to ask whether the app is connected. Until a source is set the deck counts as offline. While
 * it is offline, every app and widget tile draws at the theme's dim_opa and a tap on one sends nothing. The deck asks
 * on its widget redraw timer, so a change shows within one redraw. */
void ui_set_link_online(bool (*online)(void));

/* True when the online source says the app is connected. */
bool ui_link_online(void);

/* Writes a widget tile's state and marks it for the next redraw, from any task and without the LVGL lock. Returns
 * NULL and the stored state in *stored (when not NULL), or the refusal: "unknown tile", "display not ready". */
const char *ui_widget_update(const char *tile, const char *value, const char *caption, float bar, bool stale,
                             ui_widget_state_t *stored);

/* Replaces a dashboard page's rows the same way. Refusals: "unknown page", "too many rows", "display not ready". */
const char *ui_dashboard_update(const char *page, const ui_dash_row_t *rows, size_t count);

/* The ui console commands, for td_console_register(). Before the display has started, a well-formed command
 * answers "display not ready". */
const esp_console_cmd_t *ui_console_commands(size_t *count);

#ifdef __cplusplus
}
#endif
