#include "ui_live_now_playing.h"

void ui_live_np_feed(ui_live_np_t *np, uint32_t generation, const ui_live_slot_t *state)
{
    np->generation = generation;
    np->state = state != NULL && state->set && !state->stale ? ui_np_state(ui_live_latest(state)) : UI_NP_IDLE;
}

ui_part_t ui_live_np_part(const ui_live_np_t *np, uint32_t generation, const ui_np_layout_t *layout, int16_t x,
                          int16_t y)
{
    return ui_np_hit(layout, x, y, np->generation == generation ? np->state : UI_NP_IDLE);
}
