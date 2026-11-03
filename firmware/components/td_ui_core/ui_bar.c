#include "ui_bar.h"
#include <math.h>
#include <stdlib.h>

static int16_t max16(int16_t a, int16_t b)
{
    return a > b ? a : b;
}

static int16_t min16(int16_t a, int16_t b)
{
    return a < b ? a : b;
}

ui_bar_layout_t ui_bar_layout(int16_t w, int16_t h, bool muted_declared)
{
    ui_bar_layout_t layout = { 0 };
    layout.glyph = (ui_rect_t){ UI_BAR_PAD, UI_BAR_PAD, UI_BAR_GLYPH, UI_BAR_GLYPH };
    layout.track = (ui_rect_t){ UI_BAR_PAD, (int16_t)(h - UI_BAR_PAD - UI_BAR_TRACK_H),
                                max16((int16_t)(w - 2 * UI_BAR_PAD), 0), UI_BAR_TRACK_H };
    layout.zones = w >= 2 * UI_BAR_PAD + UI_BAR_GLYPH &&
                   h >= 2 * UI_BAR_PAD + (UI_BAR_GLYPH > UI_BAR_TRACK_H ? UI_BAR_GLYPH : UI_BAR_TRACK_H);
    if (!layout.zones) {
        return layout;
    }
    const int16_t top = max16((int16_t)(layout.track.y - UI_BAR_CAPTURE_Y), UI_BAR_PAD);
    const int16_t bottom =
        min16((int16_t)(layout.track.y + UI_BAR_TRACK_H + UI_BAR_CAPTURE_Y), (int16_t)(h - UI_BAR_PAD));
    layout.capture = (ui_rect_t){ layout.track.x, top, layout.track.w, (int16_t)(bottom - top) };
    if (muted_declared) {
        layout.mute = layout.glyph;
    }
    return layout;
}

static bool inside(ui_rect_t r, int16_t x, int16_t y)
{
    return r.w > 0 && r.h > 0 && x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h;
}

ui_bar_zone_t ui_bar_hit(const ui_bar_layout_t *layout, int16_t x, int16_t y, bool fresh, bool input)
{
    if (!layout->zones || !fresh) {
        return UI_BAR_ZONE_NONE;
    }
    if (inside(layout->mute, x, y)) {
        return UI_BAR_ZONE_MUTE;
    }
    return input && inside(layout->capture, x, y) ? UI_BAR_ZONE_VALUE : UI_BAR_ZONE_NONE;
}

static double clamp01(double v)
{
    return v < 0 ? 0 : v > 1 ? 1 : v;
}

double ui_bar_value_at(const ui_bar_layout_t *layout, int16_t x)
{
    if (layout->track.w <= 0) {
        return 0;
    }
    return clamp01((double)(x - layout->track.x) / layout->track.w);
}

int16_t ui_bar_fill(const ui_bar_layout_t *layout, double value)
{
    if (!isfinite(value)) {
        value = 0;
    }
    return (int16_t)lround(clamp01(value) * layout->track.w);
}

void ui_bar_init(ui_bar_t *bar)
{
    *bar = (ui_bar_t){ .phase = UI_BAR_LIVE };
}

static bool confirms(const ui_bar_t *bar)
{
    return bar->has_live && fabs(bar->live - bar->finger) <= UI_BAR_CONFIRM + 1e-9;
}

void ui_bar_live(ui_bar_t *bar, double value)
{
    bar->live = value;
    bar->has_live = true;
    if (bar->phase == UI_BAR_SETTLING && confirms(bar)) {
        bar->phase = UI_BAR_LIVE;
    }
}

void ui_bar_press(ui_bar_t *bar, int16_t x, int16_t y)
{
    bar->pressed = true;
    bar->x0 = x;
    bar->y0 = y;
}

bool ui_bar_move(ui_bar_t *bar, const ui_bar_layout_t *layout, int16_t x, int16_t y, int16_t slop)
{
    if (!bar->pressed) {
        return false;
    }
    if (bar->phase != UI_BAR_DRAGGING) {
        const int travel = abs(x - bar->x0) > abs(y - bar->y0) ? abs(x - bar->x0) : abs(y - bar->y0);
        if (travel <= slop) {
            return false;
        }
        bar->phase = UI_BAR_DRAGGING;
    }
    bar->finger = ui_bar_value_at(layout, x);
    return true;
}

bool ui_bar_release(ui_bar_t *bar, uint32_t now_ms)
{
    const bool dragged = bar->pressed && bar->phase == UI_BAR_DRAGGING;
    bar->pressed = false;
    if (!dragged) {
        return false;
    }
    bar->phase = confirms(bar) ? UI_BAR_LIVE : UI_BAR_SETTLING;
    bar->since_ms = now_ms;
    return true;
}

bool ui_bar_tick(ui_bar_t *bar, uint32_t now_ms)
{
    if (bar->phase != UI_BAR_SETTLING || (uint32_t)(now_ms - bar->since_ms) < UI_BAR_SETTLE_MS) {
        return false;
    }
    bar->phase = UI_BAR_LIVE;
    return true;
}

void ui_bar_cancel(ui_bar_t *bar)
{
    bar->pressed = false;
    bar->phase = UI_BAR_LIVE;
}

double ui_bar_shown(const ui_bar_t *bar)
{
    if (bar->phase != UI_BAR_LIVE) {
        return bar->finger;
    }
    return bar->has_live ? bar->live : 0;
}
