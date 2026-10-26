#pragma once

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
 * as the screen less the title strip above it and the dot strip below it. The grid sits inside the band with a
 * margin. The title, the dot row, the gap, the slop and the swipe threshold scale with the short side, which is 600
 * in both orientations. The dashboard's value and bar columns sit at the same fractions of the grid width. Landscape
 * gives the milestone 1 pixel values exactly.
 */
typedef struct {
    ui_size_t screen;
    ui_rect_t band;
    ui_rect_t grid;
    int16_t gap;
    int16_t title_x;
    int16_t title_y;
    int16_t dot_y;
    int16_t dot_pitch;
    int16_t dot_size;
    int16_t dot_active_extra;
    int16_t bounce_px;
    int16_t slop;
    int16_t swipe;
    int16_t dash_value_x;
    int16_t dash_bar_x;
} ui_geometry_t;

/* 1024 by 600 in landscape and 600 by 1024 in portrait. */
ui_size_t ui_screen_size(ui_orientation_t orientation);

ui_geometry_t ui_geometry(ui_size_t screen);

/* The x of the centre of dot `index` when `count` dots sit in a row centred on the screen. */
int16_t ui_geometry_dot_x(const ui_geometry_t *g, uint8_t index, uint8_t count);

/*
 * A point the touch controller reported on the landscape panel, in screen coordinates. Portrait undoes the display's
 * 90 degree clockwise rotation, which draws screen (x, y) at panel (1023 - y, x), so the panel's top-left corner is
 * the screen's bottom-left. The point is clamped to the panel first.
 */
ui_point_t ui_touch_to_screen(ui_orientation_t orientation, int32_t panel_x, int32_t panel_y);

#ifdef __cplusplus
}
#endif
