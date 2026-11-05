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

bool ui_live_bar_feed(ui_live_bar_t *bar, uint32_t generation, const ui_live_slot_t *value)
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

bool ui_live_bar_press(ui_live_bar_t *bar, uint32_t generation, int16_t x, int16_t y)
{
    if (!current(bar, generation)) {
        return false;
    }
    ui_bar_press(&bar->bar, x, y);
    return true;
}

bool ui_live_bar_move(ui_live_bar_t *bar, uint32_t generation, const ui_bar_layout_t *layout, int16_t x, int16_t y,
                      int16_t slop)
{
    return current(bar, generation) && ui_bar_move(&bar->bar, layout, x, y, slop);
}

bool ui_live_bar_release(ui_live_bar_t *bar, uint32_t generation, uint32_t now_ms)
{
    return current(bar, generation) && ui_bar_release(&bar->bar, now_ms);
}

ui_part_t ui_live_bar_part(const ui_live_bar_t *bar, uint32_t generation, const ui_bar_layout_t *layout, int16_t x,
                           int16_t y)
{
    const bool fresh = bar->generation == generation && bar->fresh;
    return ui_bar_hit(layout, x, y, fresh, false) == UI_BAR_ZONE_MUTE ? UI_PART_MUTE : UI_PART_NONE;
}
