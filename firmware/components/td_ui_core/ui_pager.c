#include "ui_pager.h"

static int32_t abs_i32(int32_t v)
{
    return v < 0 ? -v : v;
}

void ui_pager_init(ui_pager_t *p, const ui_pager_cfg_t *cfg, uint8_t count)
{
    *p = (ui_pager_t){ .cfg = *cfg, .count = count > 0 ? count : 1, .press = UI_PRESS_NONE };
}

void ui_pager_press(ui_pager_t *p, int16_t x, int16_t y)
{
    p->press = UI_PRESS_TAP;
    p->x0 = x;
    p->y0 = y;
}

ui_pager_move_t ui_pager_move(ui_pager_t *p, int16_t x, int16_t y)
{
    ui_pager_move_t m = { .from = p->index };
    const int32_t dx = (int32_t)x - p->x0;
    const int32_t dy = (int32_t)y - p->y0;
    const int32_t ax = abs_i32(dx);
    const int32_t ay = abs_i32(dy);

    if (p->press == UI_PRESS_TAP && (ax > p->cfg.slop || ay > p->cfg.slop)) {
        p->press = ax >= ay ? UI_PRESS_DRAG : UI_PRESS_MOVED;
    }
    if (p->press == UI_PRESS_DRAG && ax >= p->cfg.swipe) {
        p->press = UI_PRESS_SWIPED;
        if (dx < 0 && p->index + 1 < p->count) {
            p->index++;
        } else if (dx > 0 && p->index > 0) {
            p->index--;
        } else {
            m.blocked = dx < 0 ? -1 : 1;
        }
    }
    m.press = p->press;
    m.to = p->index;
    m.changed = m.to != m.from;
    return m;
}

bool ui_pager_release(ui_pager_t *p)
{
    const bool tap = p->press == UI_PRESS_TAP;
    p->press = UI_PRESS_NONE;
    return tap;
}

int16_t ui_pager_rest_x(const ui_pager_t *p)
{
    return (int16_t)(-(int32_t)p->index * p->cfg.page_w);
}

bool ui_pager_goto(ui_pager_t *p, uint8_t index)
{
    if (index >= p->count) {
        return false;
    }
    p->index = index;
    p->press = UI_PRESS_NONE;
    return true;
}
