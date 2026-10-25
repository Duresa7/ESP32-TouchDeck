#pragma once

#include <stdbool.h>
#include <stdint.h>
#include "ui_pages.h"

#define UI_PAGES_MAX 8

/*
 * What the console reports about the deck. Totals count the page changes that ran their whole slide, so
 * transition_frames * 1000 / transition_ms is the frame rate during a transition. A slide that a new touch cut
 * short and a bounce at an end count in neither the totals nor the `last_` fields.
 */
typedef struct {
    uint8_t page;
    uint8_t pages;
    const char *title;
    const char *pressed_tile;
    uint32_t taps;
    bool animating;
    uint32_t transitions;
    uint32_t transition_ms;
    uint32_t transition_frames;
    uint32_t last_ms;
    uint32_t last_frames;
} ui_state_t;

/* Reads the deck's state without the LVGL lock. Valid once ui_build() has run. */
void ui_get_state(ui_state_t *out);

/* Slides to a page, as a swipe would. False when the page does not exist. The caller holds the LVGL lock. */
bool ui_goto(uint8_t page);

/* Where a tile sits on its page, which is where it sits on the screen while that page shows. */
bool ui_find_tile(const char *id, uint8_t *page, ui_rect_t *rect);
