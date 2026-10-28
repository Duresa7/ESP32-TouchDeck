#include "ui_geometry.h"

/* docs/design.md draws the screen for a short side of 600 px, and the dashboard columns for a 968 px grid. */
#define DESIGN_SHORT 600
#define DESIGN_DASH_W 968

/* The Geometry table of docs/design.md. The band reaches BAND_MARGIN past the grid above and below. */
#define PAD_X 28
#define PAD_TOP 76
#define PAD_BOTTOM 56
#define BAND_MARGIN 20
#define GAP 16
#define TITLE_X 36
#define TITLE_Y 22
#define PAGER_DOT 12
#define PAGER_CURRENT 36
#define PAGER_GAP 12
#define PAGER_BOTTOM 18

static int16_t scaled(int32_t px, int32_t short_side)
{
    return (int16_t)(px * short_side / DESIGN_SHORT);
}

ui_size_t ui_screen_size(ui_orientation_t orientation)
{
    return orientation == UI_PORTRAIT ? (ui_size_t){ UI_PANEL_H, UI_PANEL_W } : (ui_size_t){ UI_PANEL_W, UI_PANEL_H };
}

ui_geometry_t ui_geometry(ui_size_t screen, bool full_screen)
{
    const int32_t s = screen.w < screen.h ? screen.w : screen.h;
    const int16_t title_strip = scaled(PAD_TOP - BAND_MARGIN, s);
    const int16_t pager_strip = scaled(PAD_BOTTOM - BAND_MARGIN, s);
    const int16_t margin_x = scaled(PAD_X, s);
    const int16_t margin_y = scaled(BAND_MARGIN, s);

    ui_geometry_t g = {
        .screen = screen,
        .band = { .x = 0, .y = title_strip, .w = screen.w, .h = (int16_t)(screen.h - title_strip - pager_strip) },
        .gap = scaled(GAP, s),
        .title_x = scaled(TITLE_X, s),
        .title_y = scaled(TITLE_Y, s),
        .dot_y = (int16_t)(screen.h - scaled(PAGER_BOTTOM + PAGER_DOT / 2, s)),
        .dot_size = scaled(PAGER_DOT, s),
        .dot_current = scaled(PAGER_CURRENT, s),
        .dot_gap = scaled(PAGER_GAP, s),
        .bounce_px = scaled(48, s),
        .slop = scaled(16, s),
        .swipe = scaled(96, s),
    };
    g.grid = (ui_rect_t){ .x = margin_x, .y = (int16_t)(g.band.y + margin_y), .w = (int16_t)(screen.w - 2 * margin_x),
                          .h = (int16_t)(g.band.h - 2 * margin_y) };
    if (full_screen) {
        g.band = (ui_rect_t){ .x = 0, .y = 0, .w = screen.w, .h = screen.h };
        g.grid = g.band;
    }
    g.dash_value_x = (int16_t)(g.grid.w * 360 / DESIGN_DASH_W);
    g.dash_bar_x = (int16_t)(g.grid.w * 600 / DESIGN_DASH_W);
    return g;
}

ui_rect_t ui_geometry_pager_dot(const ui_geometry_t *g, uint8_t index, uint8_t count, uint8_t current)
{
    const int32_t pitch = g->dot_size + g->dot_gap;
    const int32_t row_w = (count > 0 ? count - 1 : 0) * pitch + g->dot_current;
    int32_t x = (g->screen.w - row_w) / 2 + index * pitch;
    if (index > current) {
        x += g->dot_current - g->dot_size;
    }
    return (ui_rect_t){
        .x = (int16_t)x,
        .y = (int16_t)(g->dot_y - g->dot_size / 2),
        .w = index == current ? g->dot_current : g->dot_size,
        .h = g->dot_size,
    };
}

static int32_t clamp(int32_t v, int32_t size)
{
    return v < 0 ? 0 : v >= size ? size - 1 : v;
}

ui_point_t ui_touch_to_screen(ui_orientation_t orientation, int32_t panel_x, int32_t panel_y)
{
    const int32_t x = clamp(panel_x, UI_PANEL_W);
    const int32_t y = clamp(panel_y, UI_PANEL_H);
    if (orientation == UI_PORTRAIT) {
        return (ui_point_t){ .x = (int16_t)y, .y = (int16_t)(UI_PANEL_W - 1 - x) };
    }
    return (ui_point_t){ .x = (int16_t)x, .y = (int16_t)y };
}
