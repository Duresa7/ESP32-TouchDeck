#include "ui_geometry.h"

/* The milestone 1 design was drawn for a short side of 600 px, and the dashboard columns for a 968 px grid. */
#define DESIGN_SHORT 600
#define DESIGN_DASH_W 968

static int16_t scaled(int32_t px, int32_t short_side)
{
    return (int16_t)(px * short_side / DESIGN_SHORT);
}

ui_size_t ui_screen_size(ui_orientation_t orientation)
{
    return orientation == UI_PORTRAIT ? (ui_size_t){ UI_PANEL_H, UI_PANEL_W } : (ui_size_t){ UI_PANEL_W, UI_PANEL_H };
}

ui_geometry_t ui_geometry(ui_size_t screen)
{
    const int32_t s = screen.w < screen.h ? screen.w : screen.h;
    const int16_t title_strip = scaled(56, s);
    const int16_t dot_strip = scaled(40, s);
    const int16_t margin_x = scaled(28, s);
    const int16_t margin_y = scaled(20, s);

    ui_geometry_t g = {
        .screen = screen,
        .band = { .x = 0, .y = title_strip, .w = screen.w, .h = (int16_t)(screen.h - title_strip - dot_strip) },
        .gap = scaled(16, s),
        .title_x = scaled(36, s),
        .title_y = scaled(14, s),
        .dot_y = (int16_t)(screen.h - scaled(20, s)),
        .dot_pitch = scaled(34, s),
        .dot_size = scaled(12, s),
        .dot_active_extra = scaled(11, s),
        .bounce_px = scaled(48, s),
        .slop = scaled(16, s),
        .swipe = scaled(96, s),
    };
    g.grid = (ui_rect_t){ .x = margin_x, .y = (int16_t)(g.band.y + margin_y), .w = (int16_t)(screen.w - 2 * margin_x),
                          .h = (int16_t)(g.band.h - 2 * margin_y) };
    g.dash_value_x = (int16_t)(g.grid.w * 360 / DESIGN_DASH_W);
    g.dash_bar_x = (int16_t)(g.grid.w * 600 / DESIGN_DASH_W);
    return g;
}

int16_t ui_geometry_dot_x(const ui_geometry_t *g, uint8_t index, uint8_t count)
{
    const int span = (count > 0 ? count - 1 : 0) * g->dot_pitch;
    return (int16_t)((g->screen.w - span) / 2 + index * g->dot_pitch);
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
