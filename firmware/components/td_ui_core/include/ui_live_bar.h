#pragma once

#include <stdbool.h>
#include <stdint.h>
#include "ui_bar.h"
#include "ui_live.h"

#ifdef __cplusplus
extern "C" {
#endif

/*
 * A bar Tile as the screen drives it from its `value` slot. `generation` is the live table's generation (see
 * ui_live_table_t) that the bar's drag and its fed value belong to. Each call below takes the table's generation as
 * it is now, and a bar from an older one starts over first: the press or drag it held ends without a move or an end,
 * and the value it was fed is forgotten. `fed_n` and `fed_digest` name the slot value it was fed last, so each value is
 * fed once.
 */
typedef struct {
    ui_bar_t bar;
    uint32_t generation;
    uint32_t fed_n;
    uint32_t fed_digest;
} ui_live_bar_t;

/* No finger and no value, in `generation`. */
void ui_live_bar_init(ui_live_bar_t *bar, uint32_t generation);

/* The `value` slot as it was copied at `generation`, or NULL for a Tile without one. A set value the bar was not fed
 * yet becomes its live value. An unset one clears what it was fed, so the next set value is fed whatever its `n`. */
void ui_live_bar_feed(ui_live_bar_t *bar, uint32_t generation, const ui_live_slot_t *value);

/* A press in the value zone. False, holding nothing, when the bar was from an older generation, because what the
 * screen drew for it is out of date. */
bool ui_live_bar_press(ui_live_bar_t *bar, uint32_t generation, int16_t x, int16_t y);

/* ui_bar_move in `generation`. A press or drag from an older generation sends nothing. */
bool ui_live_bar_move(ui_live_bar_t *bar, uint32_t generation, const ui_bar_layout_t *layout, int16_t x, int16_t y,
                      int16_t slop);

/* ui_bar_release in `generation`. A drag from an older generation sends no end. */
bool ui_live_bar_release(ui_live_bar_t *bar, uint32_t generation, uint32_t now_ms);

#ifdef __cplusplus
}
#endif
