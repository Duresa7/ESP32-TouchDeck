#include "ui_live_bar.h"

void ui_live_bar_init(ui_live_bar_t *bar, uint32_t generation)
{
    *bar = (ui_live_bar_t){ .generation = generation };
    ui_bar_init(&bar->bar);
}

static bool current(ui_live_bar_t *bar, uint32_t generation)
{
    if (bar->generation == generation) {
        return true;
    }
    ui_live_bar_init(bar, generation);
    return false;
}

/* True when a value that is not fresh ended the press, drag or settle the bar held. */
static bool bar_feed(ui_live_bar_t *bar, uint32_t generation, const ui_live_slot_t *value)
{
    current(bar, generation);
    bar->fresh = value != NULL && value->set && !value->stale;
    if (value == NULL || !value->set) {
        bar->fed_n = 0;
        bar->fed_digest = 0;
    } else if (value->n != bar->fed_n || value->digest != bar->fed_digest) {
        bar->fed_n = value->n;
        bar->fed_digest = value->digest;
        ui_bar_live(&bar->bar, ui_live_latest(value));
    }
    if (bar->fresh || (!bar->bar.pressed && bar->bar.phase == UI_BAR_LIVE)) {
        return false;
    }
    ui_bar_cancel(&bar->bar);
    return true;
}

void ui_live_touch_init(ui_live_touch_t *touch, const ui_live_touch_out_t *out)
{
    *touch = (ui_live_touch_t){ .out = out };
}

static void send_input(const ui_live_touch_t *touch, double value, bool end)
{
    if (touch->out != NULL && touch->out->input != NULL) {
        touch->out->input(touch->out->ctx, touch->page, touch->tile, value, end);
    }
}

static void cancel_input(const ui_live_touch_t *touch)
{
    if (touch->out != NULL && touch->out->cancel != NULL) {
        touch->out->cancel(touch->out->ctx);
    }
}

static bool send_part(const ui_live_touch_t *touch, const char *page, const ui_tile_t *tile, ui_part_t part,
                      uint32_t generation)
{
    const char *name = ui_part_name(part);
    if (name == NULL) {
        return false;
    }
    if (touch->out != NULL && touch->out->part != NULL) {
        touch->out->part(touch->out->ctx, page, tile, name, generation);
    }
    return true;
}

static void forget(ui_live_touch_t *touch)
{
    touch->held = NULL;
    touch->layout = NULL;
    touch->page = NULL;
    touch->tile = NULL;
}

void ui_live_touch_reset(ui_live_touch_t *touch)
{
    forget(touch);
    cancel_input(touch);
}

bool ui_live_touch_press(ui_live_touch_t *touch, ui_live_bar_t *bar, const ui_bar_layout_t *layout, const char *page,
                         const ui_tile_t *tile, uint32_t generation, int16_t x, int16_t y)
{
    if (touch->held != NULL) {
        ui_bar_cancel(&touch->held->bar);
        forget(touch);
        cancel_input(touch);
    }
    if (bar == NULL || ui_bar_hit(layout, x, y, bar->fresh, tile->input) != UI_BAR_ZONE_VALUE ||
        !current(bar, generation)) {
        return false;
    }
    ui_bar_press(&bar->bar, x, y);
    *touch = (ui_live_touch_t){ .out = touch->out, .held = bar, .layout = layout, .page = page, .tile = tile->id };
    return true;
}

ui_live_bar_t *ui_live_touch_move(ui_live_touch_t *touch, uint32_t generation, int16_t x, int16_t y, int16_t slop)
{
    ui_live_bar_t *bar = touch->held;
    if (bar == NULL || !current(bar, generation) || !ui_bar_move(&bar->bar, touch->layout, x, y, slop)) {
        return NULL;
    }
    send_input(touch, bar->bar.finger, false);
    return bar;
}

ui_live_bar_t *ui_live_touch_release(ui_live_touch_t *touch, uint32_t generation, uint32_t now_ms)
{
    ui_live_bar_t *bar = touch->held;
    if (bar == NULL) {
        return NULL;
    }
    const bool ended = current(bar, generation) && ui_bar_release(&bar->bar, now_ms);
    if (ended) {
        send_input(touch, bar->bar.finger, true);
    }
    forget(touch);
    return ended ? bar : NULL;
}

void ui_live_touch_feed(ui_live_touch_t *touch, ui_live_bar_t *bar, uint32_t generation, const ui_live_slot_t *value)
{
    if (bar_feed(bar, generation, value) && bar == touch->held) {
        forget(touch);
        cancel_input(touch);
    }
}

bool ui_live_touch_holds(const ui_live_touch_t *touch, const ui_live_bar_t *bar)
{
    return bar != NULL && touch->held == bar;
}

bool ui_live_touch_tap_bar(const ui_live_touch_t *touch, const ui_live_bar_t *bar, const ui_bar_layout_t *layout,
                           const char *page, const ui_tile_t *tile, uint32_t generation, int16_t x, int16_t y)
{
    const bool fresh = bar->generation == generation && bar->fresh;
    const ui_part_t part = ui_bar_hit(layout, x, y, fresh, false) == UI_BAR_ZONE_MUTE ? UI_PART_MUTE : UI_PART_NONE;
    return send_part(touch, page, tile, part, generation);
}

bool ui_live_touch_tap_np(const ui_live_touch_t *touch, const ui_live_np_t *np, const ui_np_layout_t *layout,
                          const char *page, const ui_tile_t *tile, uint32_t generation, int16_t x, int16_t y)
{
    return send_part(touch, page, tile, ui_live_np_part(np, generation, layout, x, y), generation);
}
