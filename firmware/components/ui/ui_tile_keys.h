#pragma once

#include "lvgl.h"
#include "ui_pages.h"

/* Every call has the LVGL lock, and takes the engine's lock inside it, never the other way round. */

/* Call before the tiles of a layout are built. The modifier subjects are made once, and the poll timer of the last
 * build is deleted and made again. */
void ui_tile_keys_start(void);

/* After a tile is built: a modifier tile gets its three looks and the lock glyph. Other tiles are left alone. */
void ui_tile_keys_dress(lv_obj_t *obj, const ui_tile_t *tile);

/* The tile on `page` was tapped: runs its action, then syncs the looks. A key, modifier or consumer action goes to
 * the engine, a page action slides to its page, and an app or widget action goes to the press hook, which
 * the link turns into a `press` event, and logs `ui: press <page>/<tile> <kind>` when no hook is set. */
void ui_tile_keys_click(const ui_page_t *page, const ui_tile_t *tile);

/* The page changed: one-shot modifiers return to OFF and locked ones stay. */
void ui_tile_keys_page_changed(void);
