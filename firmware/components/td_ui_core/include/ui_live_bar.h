#pragma once

#include <stdbool.h>
#include <stdint.h>
#include "ui_bar.h"
#include "ui_live.h"
#include "ui_live_now_playing.h"
#include "ui_now_playing.h"

#ifdef __cplusplus
extern "C" {
#endif

/*
 * A bar Tile as the screen drives it from its `value` slot. `generation` is the live table's generation (see
 * ui_live_table_t) that the bar's drag and its fed value belong to. A bar from an older generation starts over at
 * the next touch call or feed: the press or drag it held ends without a move or an end, and the value it was fed is
 * forgotten. `fresh` is true while that value is set and not stale. `fed_n` and `fed_digest` name the slot value it
 * was fed last, so each value is fed once.
 */
typedef struct {
    ui_bar_t bar;
    uint32_t generation;
    bool fresh;
    uint32_t fed_n;
    uint32_t fed_digest;
} ui_live_bar_t;

/* No finger and no value, in `generation`. */
void ui_live_bar_init(ui_live_bar_t *bar, uint32_t generation);

/*
 * Where a Live tile touch goes. The Device wires these to the Companion link. None may block.
 * - `input` queues a bar drag's value, 0 to 1: once for each move past the slop, then once with `end`.
 * - `cancel` drops the queued input that has not gone out.
 * - `part` queues a tap on a part of `tile`, named by a value fed in `generation`. The queue must drop the tap when the
 *   live table has left `generation` by the time it would go out.
 */
typedef struct {
    void *ctx;
    void (*input)(void *ctx, const char *page, const char *tile, double value, bool end);
    void (*cancel)(void *ctx);
    void (*part)(void *ctx, const char *page, const ui_tile_t *tile, const char *part, uint32_t generation);
} ui_live_touch_out_t;

/*
 * The finger on the screen's Live tiles, from finger-down to what it queued for the Companion. `held` is the bar a
 * press holds, with its layout and the Page and Tile its input names, until the finger lifts, its value goes stale,
 * another press starts or the touch resets.
 *
 * Every call takes the live table's generation as it is now, and a bar from an older generation sends nothing. The
 * caller serializes the calls that change `held` (press, move, release, feed and reset), the Device under its
 * live-value lock. The taps read only the bar or Tile they are given.
 */
typedef struct {
    const ui_live_touch_out_t *out;
    ui_live_bar_t *held;
    const ui_bar_layout_t *layout;
    const char *page;
    const char *tile;
} ui_live_touch_t;

/* Holds nothing, and sends to `out`. */
void ui_live_touch_init(ui_live_touch_t *touch, const ui_live_touch_out_t *out);

/* A new session or Layout: forgets the bar it held without touching it, and drops the queued input. The bar's own
 * generation ends its gesture when the screen next touches or feeds it. */
void ui_live_touch_reset(ui_live_touch_t *touch);

/* A finger down at Tile point (x, y) on `bar`, or on no bar when it is NULL. A gesture still held ends first, and its
 * queued input is dropped. The bar holds the finger, and the call returns true, when the point is in the value zone of
 * a bar fed a fresh value that takes input, and the bar is of `generation`. */
bool ui_live_touch_press(ui_live_touch_t *touch, ui_live_bar_t *bar, const ui_bar_layout_t *layout, const char *page,
                         const ui_tile_t *tile, uint32_t generation, int16_t x, int16_t y);

/* The held finger at Tile point (x, y). Once it passes `slop`, each move queues the finger's value. The bar that moved,
 * to redraw, or NULL. */
ui_live_bar_t *ui_live_touch_move(ui_live_touch_t *touch, uint32_t generation, int16_t x, int16_t y, int16_t slop);

/* The held finger lifted or the press was lost. A drag queues its end and settles. The bar that ended a drag, to
 * redraw, or NULL. */
ui_live_bar_t *ui_live_touch_release(ui_live_touch_t *touch, uint32_t generation, uint32_t now_ms);

/* The `value` slot as it was copied at `generation`, or NULL for a Tile without one. A set value the bar was not fed
 * yet becomes its live value. An unset one clears what it was fed, so the next set value is fed whatever its `n`.
 * A value that is not fresh ends the press, drag or settle the bar holds, so it shows the value it was fed. When that
 * bar is the held one, its gesture sends no further move or end and its queued input is dropped. */
void ui_live_touch_feed(ui_live_touch_t *touch, ui_live_bar_t *bar, uint32_t generation, const ui_live_slot_t *value);

/* True while `bar` holds the finger. */
bool ui_live_touch_holds(const ui_live_touch_t *touch, const ui_live_bar_t *bar);

/* A tap at Tile point (x, y) on a bar Tile. On the mute zone of a bar fed a fresh value in `generation`, it queues the
 * mute part with `generation` and returns true. Anywhere else it sends nothing. */
bool ui_live_touch_tap_bar(const ui_live_touch_t *touch, const ui_live_bar_t *bar, const ui_bar_layout_t *layout,
                           const char *page, const ui_tile_t *tile, uint32_t generation, int16_t x, int16_t y);

/* A tap at Tile point (x, y) on a now-playing Tile. The part ui_np_hit names for the state fed in `generation` is
 * queued with `generation`, and the call returns true. A Tile fed in an older generation sends nothing. */
bool ui_live_touch_tap_np(const ui_live_touch_t *touch, const ui_live_np_t *np, const ui_np_layout_t *layout,
                          const char *page, const ui_tile_t *tile, uint32_t generation, int16_t x, int16_t y);

#ifdef __cplusplus
}
#endif
