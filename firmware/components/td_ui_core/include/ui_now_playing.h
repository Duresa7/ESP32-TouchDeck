#pragma once

#include <stdbool.h>
#include <stdint.h>
#include "ui_pages.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The now-playing constants of schema/live/now-playing-vectors.json, in Tile pixels. */
#define UI_NP_PAD 20
#define UI_NP_ART_MAX 256
#define UI_NP_ART_MIN 48
#define UI_NP_ROW_H 56
#define UI_NP_ROW_MIN_TILE_H 200
#define UI_NP_ROW_MIN_TEXT_W 180
#define UI_NP_PARTS_MAX 3

/* A Tile part a tap can name in `press.part`. */
typedef enum {
    UI_PART_NONE = 0,
    UI_PART_PREVIOUS,
    UI_PART_PLAY_PAUSE,
    UI_PART_NEXT,
    UI_PART_MUTE,
} ui_part_t;

/* The wire name of a part, or NULL for UI_PART_NONE. */
const char *ui_part_name(ui_part_t part);

/* The `state` stream: 0 nothing playing, 1 paused, 2 playing. */
typedef enum {
    UI_NP_IDLE = 0,
    UI_NP_PAUSED = 1,
    UI_NP_PLAYING = 2,
} ui_np_state_t;

/* A `state` value as the Tile draws it. 1 is paused and 2 playing; anything else, a fraction included, is idle. */
ui_np_state_t ui_np_state(double value);

/*
 * Where a now-playing Tile of w by h draws, from the vectors. `art` is the cover's square, `pad` from the left and
 * centred vertically, and `has_art` is false on a Tile too small for one. The text column runs from text_x for text_w
 * pixels. With `controls`, the previous, play_pause and next zones split the column's width in a row at the bottom;
 * without it, the one zone is the whole Tile and taps play_pause. A zone holds its top and left edges and not its bottom
 * and right ones.
 */
typedef struct {
    bool has_art;
    ui_rect_t art;
    int16_t text_x;
    int16_t text_w;
    bool controls;
    uint8_t part_count;
    ui_part_t parts[UI_NP_PARTS_MAX];
    ui_rect_t zones[UI_NP_PARTS_MAX];
} ui_np_layout_t;

ui_np_layout_t ui_np_layout(int16_t w, int16_t h);

/* The part a tap at Tile point (x, y) names. Nothing playing names none. */
ui_part_t ui_np_hit(const ui_np_layout_t *layout, int16_t x, int16_t y, ui_np_state_t state);

#ifdef __cplusplus
}
#endif
