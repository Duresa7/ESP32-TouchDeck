#pragma once

#include <stdbool.h>
#include <stdint.h>
#include "ui_pages.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The bar constants of schema/live/bar-vectors.json, in Tile pixels. */
#define UI_BAR_PAD 20
#define UI_BAR_GLYPH 32
#define UI_BAR_TRACK_H 28
#define UI_BAR_CAPTURE_Y 24
/* A live value this close to the finger's confirms a drag, and a drag that waits longer gives up and shows the
 * newest live value. */
#define UI_BAR_CONFIRM 0.01
#define UI_BAR_SETTLE_MS 1500

/*
 * Where a bar Tile of w by h draws, from the bar vectors. The speaker glyph sits at (pad, pad) and the track spans the
 * padded width, track_h tall, pad above the bottom. `capture` is the track widened by capture_y above and below and
 * clipped to the padded Tile: a press there drags the value. `mute` is the glyph's tap zone, empty (w and h of 0) when
 * the Tile declares no `muted` stream. `zones` is false for a Tile too small for them, which then takes no drag and no
 * mute tap and still draws its glyph and track; its capture and mute are empty. A rect holds its top and left edges and not its bottom and right ones.
 */
typedef struct {
    bool zones;
    ui_rect_t glyph;
    ui_rect_t track;
    ui_rect_t capture;
    ui_rect_t mute;
} ui_bar_layout_t;

typedef enum {
    UI_BAR_ZONE_NONE = 0,
    UI_BAR_ZONE_VALUE,
    UI_BAR_ZONE_MUTE,
} ui_bar_zone_t;

ui_bar_layout_t ui_bar_layout(int16_t w, int16_t h, bool muted_declared);

/* What a press at Tile point (x, y) lands on. `fresh` is a value that is set, not stale and online; without it nothing
 * is live. The value zone needs `input`; the mute zone wins where the two overlap. */
ui_bar_zone_t ui_bar_hit(const ui_bar_layout_t *layout, int16_t x, int16_t y, bool fresh, bool input);

/* The value a finger at Tile x sets: its place along the track, 0 at the left edge and 1 at the right, clamped. */
double ui_bar_value_at(const ui_bar_layout_t *layout, int16_t x);

/* The fill width, in pixels, of `value` on the track, rounded, with the value clamped to 0 to 1. */
int16_t ui_bar_fill(const ui_bar_layout_t *layout, double value);

/*
 * A bar's drag, after link-v2 "Touch input events". LIVE draws the newest live value. A press in the value zone is
 * `pressed` but still a tap; once the finger leaves the slop radius the bar is DRAGGING and draws the finger's value.
 * A lift moves it to SETTLING, which keeps drawing the finger's value until a live value within UI_BAR_CONFIRM of it
 * arrives or UI_BAR_SETTLE_MS pass, and then it is LIVE again. Live values that arrive meanwhile are kept, not drawn.
 */
typedef enum {
    UI_BAR_LIVE = 0,
    UI_BAR_DRAGGING,
    UI_BAR_SETTLING,
} ui_bar_phase_t;

typedef struct {
    ui_bar_phase_t phase;
    bool pressed;
    int16_t x0;
    int16_t y0;
    double finger;
    double live;
    bool has_live;
    uint32_t since_ms;
} ui_bar_t;

/* No finger, no live value. */
void ui_bar_init(ui_bar_t *bar);

/* A live value the Tile now holds. In SETTLING it confirms the drag when it is within UI_BAR_CONFIRM of the finger. */
void ui_bar_live(ui_bar_t *bar, double value);

/* A press in the value zone at Tile point (x, y). */
void ui_bar_press(ui_bar_t *bar, int16_t x, int16_t y);

/* The finger is at Tile point (x, y). True when the bar is dragging and the finger's value is one to send. A press
 * starts dragging once the larger of its travels from the press exceeds `slop`. */
bool ui_bar_move(ui_bar_t *bar, const ui_bar_layout_t *layout, int16_t x, int16_t y, int16_t slop);

/* The finger lifted, or the press was lost. True when it ended a drag, so the caller sends the end with the finger's
 * value; a press that never dragged sends nothing. */
bool ui_bar_release(ui_bar_t *bar, uint32_t now_ms);

/* Ends SETTLING once UI_BAR_SETTLE_MS have passed since the lift. True when that changed what the bar draws. */
bool ui_bar_tick(ui_bar_t *bar, uint32_t now_ms);

/* Drops a press or drag in progress, for a Layout or session change: the bar is LIVE again. */
void ui_bar_cancel(ui_bar_t *bar);

/* The value the bar draws: the live value when LIVE, the finger's otherwise. */
double ui_bar_shown(const ui_bar_t *bar);

#ifdef __cplusplus
}
#endif
