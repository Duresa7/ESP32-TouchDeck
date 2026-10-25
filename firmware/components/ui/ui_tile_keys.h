#pragma once

#include "lvgl.h"
#include "ui_pages.h"

/* Every call has the LVGL lock, and takes the engine's lock inside it, never the other way round. */

/* Call before any tile is built. */
void ui_tile_keys_start(void);

/* After a tile is built: a modifier tile gets its three looks and the lock glyph. Other tiles are left alone. */
void ui_tile_keys_dress(lv_obj_t *obj, const ui_tile_t *tile);

/* The tile was tapped: runs its action through the engine, then syncs the looks. */
void ui_tile_keys_click(const ui_tile_t *tile);

/* The page changed: one-shot modifiers return to OFF and locked ones stay. */
void ui_tile_keys_page_changed(void);
