#pragma once

#include <stdint.h>
#include "ui_live.h"
#include "ui_now_playing.h"

#ifdef __cplusplus
extern "C" {
#endif

/*
 * A now-playing Tile's taps as the screen drives them from its `state` slot. `state` is the state it was fed last,
 * idle unless that value is set and not stale, and `generation` is the live table's generation (see ui_live_table_t)
 * it was fed in. A zeroed one was fed nothing and names no part.
 */
typedef struct {
    uint32_t generation;
    ui_np_state_t state;
} ui_live_np_t;

/* The `state` slot as it was copied at `generation`, or NULL for a Tile without one. */
void ui_live_np_feed(ui_live_np_t *np, uint32_t generation, const ui_live_slot_t *state);

/* ui_np_hit with the state fed in `generation`. A Tile fed in an older generation names no part. */
ui_part_t ui_live_np_part(const ui_live_np_t *np, uint32_t generation, const ui_np_layout_t *layout, int16_t x,
                          int16_t y);

#ifdef __cplusplus
}
#endif
