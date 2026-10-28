#pragma once

#include <stdbool.h>
#include <stdint.h>
#include "ui_pages.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The panel as it scans out, landscape. Portrait is the same glass turned 90 degrees clockwise. */
#define UI_PANEL_W 1024
#define UI_PANEL_H 600

typedef enum {
    UI_LANDSCAPE = 0,
    UI_PORTRAIT,
} ui_orientation_t;

typedef struct {
    int16_t w;
    int16_t h;
} ui_size_t;

typedef struct {
    int16_t x;
    int16_t y;
} ui_point_t;

/*
 * Where the deck draws, for one screen size. The band is the strip that slides, the full screen width and as tall
 * as the screen less the title strip above it and the pager strip below it. The grid sits inside the band with a
 * margin. The title, the pager, the gap, the slop and the swipe threshold scale with the short side, which is 600
 * in both orientations. The dashboard's value and bar columns sit at the same fractions of the grid width. Landscape
 * gives the screen pixels of the Geometry table in docs/design.md exactly.
 *
 * The pager is a row of dot_size dots, dot_gap apart and centred on the screen, whose middle sits dot_y down. The
 * current Page's dot is a pill dot_current wide. dot_y leaves the pager's bottom edge the design's 18 px above the
 * screen's.
 *
 * A full-screen layout draws no title and no pager, so the band and the grid are both the whole screen, with no
 * margin. The gap, the pager distances and the title and pager fields keep their values, and the dashboard columns
 * follow the wider grid.
 */
typedef struct {
    ui_size_t screen;
    ui_rect_t band;
    ui_rect_t grid;
    int16_t gap;
    int16_t title_x;
    int16_t title_y;
    int16_t dot_y;
    int16_t dot_size;
    int16_t dot_current;
    int16_t dot_gap;
    int16_t bounce_px;
    int16_t slop;
    int16_t swipe;
    int16_t dash_value_x;
    int16_t dash_bar_x;
} ui_geometry_t;

/* 1024 by 600 in landscape and 600 by 1024 in portrait. */
ui_size_t ui_screen_size(ui_orientation_t orientation);

ui_geometry_t ui_geometry(ui_size_t screen, bool full_screen);

/* The box of pager dot `index` of `count` while Page `current` shows: a dot_size circle, or the dot_current pill for
 * the current Page. The dots after the pill sit further right by the pill's extra width, so the gaps stay dot_gap. */
ui_rect_t ui_geometry_pager_dot(const ui_geometry_t *g, uint8_t index, uint8_t count, uint8_t current);

/*
 * A point the touch controller reported on the landscape panel, in screen coordinates. Portrait undoes the display's
 * 90 degree clockwise rotation, which draws screen (x, y) at panel (1023 - y, x), so the panel's top-left corner is
 * the screen's bottom-left. The point is clamped to the panel first.
 */
ui_point_t ui_touch_to_screen(ui_orientation_t orientation, int32_t panel_x, int32_t panel_y);

#ifdef __cplusplus
}
#endif
