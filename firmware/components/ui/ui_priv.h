#pragma once

#include <stdbool.h>
#include <stdint.h>
#include "lvgl.h"
#include "td_layout.h"
#include "ui_pages.h"

#define UI_PAGES_MAX 8
#define UI_LAYOUT_ERROR_MAX 196

/*
 * What the console reports about the deck. Totals count the page changes that ran their whole slide, so
 * transition_frames * 1000 / transition_ms is the frame rate during a transition. A slide that a new touch cut
 * short and a bounce at an end count in neither the totals nor the `last_` fields. The strings are copies, so a
 * reader keeps them after the layout changes.
 */
typedef struct {
    uint8_t page;
    uint8_t pages;
    uint8_t tile_count;
    char title[TD_LAYOUT_TITLE_MAX];
    char pressed_tile[TD_LAYOUT_ID_MAX];
    uint32_t taps;
    bool animating;
    uint32_t transitions;
    uint32_t transition_ms;
    uint32_t transition_frames;
    uint32_t last_ms;
    uint32_t last_frames;
} ui_state_t;

/* One tile, copied out of the layout. `icon` is empty for a tile with no icon. */
typedef struct {
    uint8_t page;
    ui_rect_t rect;
    char label[TD_LAYOUT_LABEL_MAX];
    char action[12];
    char icon[24];
} ui_tile_info_t;

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

/* True once ui_build() has drawn a deck. */
bool ui_ready(void);

/* Copies the deck's state under the layout lock. Valid once ui_ready() is true. The totals are whole when
 * `animating` is false. */
void ui_get_state(ui_state_t *out);

/* Slides to a page, as a swipe would. False when the page does not exist. The caller holds the LVGL lock. */
bool ui_goto(uint8_t page);

/* Copies a tile and where it sits on screen while its page shows, under the layout lock. */
bool ui_tile_info(const char *id, ui_tile_info_t *out);

/* Copies what `layout state` reports, under the layout lock. */
void ui_layout_info(ui_layout_info_t *out);

/* Loads the stored layout for the boot orientation, or the built-in one with `use_builtin`, and swaps it onto the
 * screen. A candidate that fails leaves the current layout on screen and its error in `layout state`. Takes the
 * LVGL lock, so never call it from an LVGL callback. On failure *err says why. */
bool ui_layout_reload(bool use_builtin, td_layout_error_t *err);

/* The `layout` console command. */
int ui_layout_cmd(int argc, char **argv);

/* The icon cache. A file icon's bitmap stays in PSRAM, keyed by its id, so a reload of the same layout reads nothing
 * again. */
typedef struct {
    uint16_t loaded;
    uint16_t missing;
} ui_icon_counts_t;

/* Reads the `file:` icons of `layout` into the cache and counts each tile's icon as loaded or missing. It only evicts
 * icons that neither `layout` nor `keep` names, so the screen never loses an image it draws. */
ui_icon_counts_t ui_icon_prepare(const td_layout_t *layout, const td_layout_t *keep);

/* The cached image of a `file:` icon, or NULL. */
const lv_image_dsc_t *ui_icon_image(const char *icon);

/* The LVGL symbol text of an `lv:` icon, or NULL. */
const char *ui_icon_symbol(const char *icon);
