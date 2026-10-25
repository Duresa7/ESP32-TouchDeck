#pragma once

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    int16_t page_w;
    int16_t slop;
    int16_t swipe;
} ui_pager_cfg_t;

typedef enum {
    UI_PRESS_NONE,
    UI_PRESS_TAP,
    UI_PRESS_DRAG,
    UI_PRESS_MOVED,
    UI_PRESS_SWIPED,
} ui_press_t;

/*
 * The pager: which page is showing and what the finger on the glass is doing to it. The caller feeds it the
 * pointer events and acts on what it says, so nothing here knows about LVGL, time or tasks.
 *
 * - page_w is the distance in pixels between two neighbouring pages.
 * - slop is the finger travel, in either axis, that turns a press into something other than a tap.
 * - swipe is the sideways travel from the press that changes the page.
 *
 * A page changes the moment the finger has travelled far enough, with no need to wait for the lift, so the deck
 * answers a swipe while the finger is still moving. `press` is the state of the finger.
 * TAP means down and still inside the slop radius, so a click may stand.
 * DRAG means the finger left the radius sideways and has not yet gone far enough to swipe.
 * MOVED means it left the radius up or down. A MOVED press never swipes and never counts as a tap.
 * SWIPED means the swipe went far enough and was acted on. The rest of that press does nothing.
 */
typedef struct {
    ui_pager_cfg_t cfg;
    uint8_t count;
    uint8_t index;
    ui_press_t press;
    int16_t x0;
    int16_t y0;
} ui_pager_t;

typedef struct {
    ui_press_t press;
    bool changed;
    uint8_t from;
    uint8_t to;
    int8_t blocked;
} ui_pager_move_t;

/* Starts on page 0 with no finger down. A count of 0 is treated as 1. */
void ui_pager_init(ui_pager_t *p, const ui_pager_cfg_t *cfg, uint8_t count);

/* A finger went down at (x, y). Starts a new press, replacing one still in progress. */
void ui_pager_press(ui_pager_t *p, int16_t x, int16_t y);

/*
 * The finger is at (x, y). Returns the press state after the move and what the move did to the page. Does
 * nothing without a press, and nothing once the press is SWIPED or MOVED.
 *
 * A TAP leaves the slop radius when the larger of the horizontal and vertical travel from the press exceeds slop.
 * It becomes DRAG when the horizontal travel is at least the vertical one, and MOVED otherwise. A DRAG follows
 * only the horizontal position and ignores the vertical one.
 *
 * A DRAG whose horizontal travel from the press reaches swipe becomes SWIPED on that move, even when it only
 * became a DRAG on that same move. A leftward travel goes to index + 1 and a rightward one to index - 1, and
 * `changed` is true with `from` and `to` set. At the last page a leftward swipe, and at the first page a rightward
 * one, changes nothing. The pages should bounce instead, so `blocked` is -1 for a leftward swipe and 1 for a
 * rightward one. `blocked` is 0 otherwise, and `from` and `to` are the current index when nothing changed.
 */
ui_pager_move_t ui_pager_move(ui_pager_t *p, int16_t x, int16_t y);

/*
 * The finger lifted. Returns true only for a press that never left the slop radius, which is a tap. The press
 * ends, and a release without a press is not a tap. The page never changes here.
 */
bool ui_pager_release(ui_pager_t *p);

/* Where the left edge of the first page sits at rest, in pixels from the screen's left edge: -index * page_w. */
int16_t ui_pager_rest_x(const ui_pager_t *p);

/* Jumps to a page and ends any press. False, and nothing changes, when index is not below count. */
bool ui_pager_goto(ui_pager_t *p, uint8_t index);

#ifdef __cplusplus
}
#endif
