#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include "unity.h"
#include "ui_pager.h"

#define START_X 512
#define START_Y 300

#define EXPECT_MOVE(got, want_press, want_changed, want_from, want_to, want_blocked) \
    do {                                                                             \
        const ui_pager_move_t m_ = (got);                                            \
        TEST_ASSERT_EQUAL((want_press), m_.press);                                   \
        TEST_ASSERT_EQUAL((want_changed), m_.changed);                               \
        TEST_ASSERT_EQUAL_UINT8((want_from), m_.from);                               \
        TEST_ASSERT_EQUAL_UINT8((want_to), m_.to);                                   \
        TEST_ASSERT_EQUAL_INT8((want_blocked), m_.blocked);                          \
    } while (0)

static const ui_pager_cfg_t k_cfg = { .page_w = 1024, .slop = 16, .swipe = 96 };

static ui_pager_t pager_on_page(uint8_t count, uint8_t index)
{
    ui_pager_t p;
    ui_pager_init(&p, &k_cfg, count);
    TEST_ASSERT_TRUE(ui_pager_goto(&p, index));
    return p;
}

static ui_pager_move_t move_by(ui_pager_t *p, int dx, int dy)
{
    return ui_pager_move(p, (int16_t)(START_X + dx), (int16_t)(START_Y + dy));
}

/* dir is -1 for a finger that leaves the slop radius to the left and 1 for one that leaves to the right. */
static void begin_drag(ui_pager_t *p, int dir)
{
    const uint8_t at = p->index;
    ui_pager_press(p, START_X, START_Y);
    EXPECT_MOVE(move_by(p, dir * (k_cfg.slop + 1), 0), UI_PRESS_DRAG, false, at, at, 0);
}

/* A sideways drag that has travelled dx from the press point by its last move. */
static ui_pager_move_t swipe(ui_pager_t *p, int dx)
{
    begin_drag(p, dx < 0 ? -1 : 1);
    return move_by(p, dx, 0);
}

TEST_CASE("a pager starts on page 0 at rest with no finger down", "[ui_pager]")
{
    ui_pager_t p;
    memset(&p, 0xA5, sizeof p);
    ui_pager_init(&p, &k_cfg, 3);

    TEST_ASSERT_EQUAL_UINT8(3, p.count);
    TEST_ASSERT_EQUAL_UINT8(0, p.index);
    TEST_ASSERT_EQUAL(UI_PRESS_NONE, p.press);
    TEST_ASSERT_EQUAL_INT16(0, ui_pager_rest_x(&p));
}

TEST_CASE("a leftward swipe goes to the next page and a rightward swipe goes back", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 0);

    EXPECT_MOVE(swipe(&p, -200), UI_PRESS_SWIPED, true, 0, 1, 0);
    TEST_ASSERT_EQUAL_UINT8(1, p.index);

    EXPECT_MOVE(swipe(&p, 200), UI_PRESS_SWIPED, true, 1, 0, 0);
    TEST_ASSERT_EQUAL_UINT8(0, p.index);
}

TEST_CASE("the page changes during the move and the release is not a tap and changes nothing", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 0);

    EXPECT_MOVE(swipe(&p, -200), UI_PRESS_SWIPED, true, 0, 1, 0);
    TEST_ASSERT_EQUAL_UINT8_MESSAGE(1, p.index, "the finger is still down");

    TEST_ASSERT_FALSE(ui_pager_release(&p));
    TEST_ASSERT_EQUAL_UINT8(1, p.index);
    TEST_ASSERT_EQUAL(UI_PRESS_NONE, p.press);
}

TEST_CASE("swipes walk through every page in order and back", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 0);

    EXPECT_MOVE(swipe(&p, -200), UI_PRESS_SWIPED, true, 0, 1, 0);
    EXPECT_MOVE(swipe(&p, -200), UI_PRESS_SWIPED, true, 1, 2, 0);
    EXPECT_MOVE(swipe(&p, 200), UI_PRESS_SWIPED, true, 2, 1, 0);
    EXPECT_MOVE(swipe(&p, 200), UI_PRESS_SWIPED, true, 1, 0, 0);
    TEST_ASSERT_EQUAL_UINT8(0, p.index);
}

TEST_CASE("a swipe past the last page or the first page changes nothing and reports blocked", "[ui_pager]")
{
    ui_pager_t last = pager_on_page(3, 2);
    EXPECT_MOVE(swipe(&last, -200), UI_PRESS_SWIPED, false, 2, 2, -1);
    TEST_ASSERT_EQUAL_UINT8(2, last.index);
    TEST_ASSERT_FALSE(ui_pager_release(&last));

    ui_pager_t first = pager_on_page(3, 0);
    EXPECT_MOVE(swipe(&first, 200), UI_PRESS_SWIPED, false, 0, 0, 1);
    TEST_ASSERT_EQUAL_UINT8(0, first.index);
    TEST_ASSERT_FALSE(ui_pager_release(&first));
}

TEST_CASE("a swipe shorter than the swipe distance leaves the page unchanged", "[ui_pager]")
{
    const int dxs[] = { -95, 95, -40, 40 };
    for (size_t i = 0; i < sizeof dxs / sizeof dxs[0]; i++) {
        ui_pager_t p = pager_on_page(3, 1);
        EXPECT_MOVE(swipe(&p, dxs[i]), UI_PRESS_DRAG, false, 1, 1, 0);
        TEST_ASSERT_FALSE(ui_pager_release(&p));
        TEST_ASSERT_EQUAL_UINT8(1, p.index);
    }
}

TEST_CASE("a swipe of exactly the swipe distance changes the page in either direction", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 1);

    EXPECT_MOVE(swipe(&p, -96), UI_PRESS_SWIPED, true, 1, 2, 0);
    EXPECT_MOVE(swipe(&p, 96), UI_PRESS_SWIPED, true, 2, 1, 0);
}

TEST_CASE("a press that stays inside the slop radius releases as a tap", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 1);
    ui_pager_press(&p, START_X, START_Y);
    EXPECT_MOVE(move_by(&p, 10, 0), UI_PRESS_TAP, false, 1, 1, 0);
    EXPECT_MOVE(move_by(&p, -10, 10), UI_PRESS_TAP, false, 1, 1, 0);

    TEST_ASSERT_TRUE(ui_pager_release(&p));
    TEST_ASSERT_EQUAL(UI_PRESS_NONE, p.press);
    TEST_ASSERT_EQUAL_UINT8(1, p.index);
}

TEST_CASE("a press is a tap up to slop and not beyond it", "[ui_pager]")
{
    const struct {
        int dx;
        int dy;
        ui_press_t want;
    } cases[] = {
        { 0, 0, UI_PRESS_TAP },
        { 16, 0, UI_PRESS_TAP },
        { -16, 0, UI_PRESS_TAP },
        { 0, 16, UI_PRESS_TAP },
        { 0, -16, UI_PRESS_TAP },
        { 16, 16, UI_PRESS_TAP },
        { -16, -16, UI_PRESS_TAP },
        { 17, 0, UI_PRESS_DRAG },
        { -17, 0, UI_PRESS_DRAG },
        { 0, 17, UI_PRESS_MOVED },
        { 0, -17, UI_PRESS_MOVED },
        { 10, 17, UI_PRESS_MOVED },
        { 16, 17, UI_PRESS_MOVED },
        { -16, 17, UI_PRESS_MOVED },
        { 16, -17, UI_PRESS_MOVED },
        { 17, 10, UI_PRESS_DRAG },
        { 17, 16, UI_PRESS_DRAG },
        { -17, 16, UI_PRESS_DRAG },
        { 17, 17, UI_PRESS_DRAG },
        { -17, 17, UI_PRESS_DRAG },
        { 17, -17, UI_PRESS_DRAG },
        { -17, -17, UI_PRESS_DRAG },
        { 18, 17, UI_PRESS_DRAG },
        { -18, -17, UI_PRESS_DRAG },
        { 17, 18, UI_PRESS_MOVED },
        { -17, -18, UI_PRESS_MOVED },
        { -40, 39, UI_PRESS_DRAG },
        { 39, 40, UI_PRESS_MOVED },
        { 95, 95, UI_PRESS_DRAG },
        { 95, 96, UI_PRESS_MOVED },
    };
    for (size_t i = 0; i < sizeof cases / sizeof cases[0]; i++) {
        ui_pager_t p = pager_on_page(3, 1);
        ui_pager_press(&p, START_X, START_Y);
        EXPECT_MOVE(move_by(&p, cases[i].dx, cases[i].dy), cases[i].want, false, 1, 1, 0);
        TEST_ASSERT_EQUAL_MESSAGE(cases[i].want, p.press, "the press state must be kept");
        TEST_ASSERT_EQUAL_MESSAGE(cases[i].want == UI_PRESS_TAP, ui_pager_release(&p), "only TAP releases as a tap");
        TEST_ASSERT_EQUAL_UINT8(1, p.index);
    }
}

TEST_CASE("a press that leaves mostly vertically never swipes and never changes the page", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 1);
    ui_pager_press(&p, START_X, START_Y);
    EXPECT_MOVE(move_by(&p, 8, 30), UI_PRESS_MOVED, false, 1, 1, 0);

    EXPECT_MOVE(move_by(&p, -300, 30), UI_PRESS_MOVED, false, 1, 1, 0);
    EXPECT_MOVE(move_by(&p, 400, 30), UI_PRESS_MOVED, false, 1, 1, 0);
    TEST_ASSERT_EQUAL_UINT8(1, p.index);

    TEST_ASSERT_FALSE(ui_pager_release(&p));
    TEST_ASSERT_EQUAL_UINT8(1, p.index);
}

TEST_CASE("a finger that leaves the slop radius and comes back is still not a tap", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 0);
    ui_pager_press(&p, START_X, START_Y);
    EXPECT_MOVE(move_by(&p, 17, 0), UI_PRESS_DRAG, false, 0, 0, 0);
    EXPECT_MOVE(move_by(&p, 0, 0), UI_PRESS_DRAG, false, 0, 0, 0);
    TEST_ASSERT_FALSE(ui_pager_release(&p));

    ui_pager_press(&p, START_X, START_Y);
    EXPECT_MOVE(move_by(&p, 0, 17), UI_PRESS_MOVED, false, 0, 0, 0);
    EXPECT_MOVE(move_by(&p, 0, 0), UI_PRESS_MOVED, false, 0, 0, 0);
    TEST_ASSERT_FALSE(ui_pager_release(&p));
    TEST_ASSERT_EQUAL_UINT8(0, p.index);
}

TEST_CASE("a drag that returns toward the press point before the swipe distance does not swipe", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 1);
    begin_drag(&p, -1);
    EXPECT_MOVE(move_by(&p, -95, 0), UI_PRESS_DRAG, false, 1, 1, 0);
    EXPECT_MOVE(move_by(&p, -30, 0), UI_PRESS_DRAG, false, 1, 1, 0);
    EXPECT_MOVE(move_by(&p, 0, 0), UI_PRESS_DRAG, false, 1, 1, 0);

    TEST_ASSERT_FALSE(ui_pager_release(&p));
    TEST_ASSERT_EQUAL_UINT8(1, p.index);
}

TEST_CASE("travel is counted from the press point so a drag that crosses over swipes the other way", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 1);
    begin_drag(&p, -1);
    EXPECT_MOVE(move_by(&p, -90, 0), UI_PRESS_DRAG, false, 1, 1, 0);
    /* 96 px from where the drag began at -17, but only 79 from the press */
    EXPECT_MOVE(move_by(&p, 79, 0), UI_PRESS_DRAG, false, 1, 1, 0);
    EXPECT_MOVE(move_by(&p, 96, 0), UI_PRESS_SWIPED, true, 1, 0, 0);
    TEST_ASSERT_EQUAL_UINT8(0, p.index);
}

TEST_CASE("a drag follows only the horizontal position", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 1);
    begin_drag(&p, -1);
    EXPECT_MOVE(move_by(&p, -60, 290), UI_PRESS_DRAG, false, 1, 1, 0);
    EXPECT_MOVE(move_by(&p, -125, -250), UI_PRESS_SWIPED, true, 1, 2, 0);
}

TEST_CASE("a move after the swipe does nothing", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 1);
    EXPECT_MOVE(swipe(&p, -200), UI_PRESS_SWIPED, true, 1, 2, 0);
    EXPECT_MOVE(move_by(&p, -400, 0), UI_PRESS_SWIPED, false, 2, 2, 0);
    EXPECT_MOVE(move_by(&p, 300, 0), UI_PRESS_SWIPED, false, 2, 2, 0);
    EXPECT_MOVE(move_by(&p, 0, 0), UI_PRESS_SWIPED, false, 2, 2, 0);
    EXPECT_MOVE(move_by(&p, 600, 200), UI_PRESS_SWIPED, false, 2, 2, 0);
    TEST_ASSERT_EQUAL_UINT8(2, p.index);

    EXPECT_MOVE(swipe(&p, -200), UI_PRESS_SWIPED, false, 2, 2, -1);
    EXPECT_MOVE(move_by(&p, -400, 0), UI_PRESS_SWIPED, false, 2, 2, 0);
    EXPECT_MOVE(move_by(&p, 400, 0), UI_PRESS_SWIPED, false, 2, 2, 0);
    TEST_ASSERT_FALSE(ui_pager_release(&p));
}

TEST_CASE("a single large sideways first move goes straight from a tap to a swipe", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 1);
    ui_pager_press(&p, START_X, START_Y);
    EXPECT_MOVE(move_by(&p, -200, 0), UI_PRESS_SWIPED, true, 1, 2, 0);
    TEST_ASSERT_FALSE(ui_pager_release(&p));

    ui_pager_press(&p, START_X, START_Y);
    EXPECT_MOVE(move_by(&p, 200, 50), UI_PRESS_SWIPED, true, 2, 1, 0);

    ui_pager_press(&p, START_X, START_Y);
    EXPECT_MOVE(move_by(&p, 95, 0), UI_PRESS_DRAG, false, 1, 1, 0);
    EXPECT_MOVE(move_by(&p, 96, 0), UI_PRESS_SWIPED, true, 1, 0, 0);

    ui_pager_press(&p, START_X, START_Y);
    EXPECT_MOVE(move_by(&p, -96, -96), UI_PRESS_SWIPED, true, 0, 1, 0);
}

TEST_CASE("a single large mostly vertical first move becomes MOVED and not a swipe", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 1);
    ui_pager_press(&p, START_X, START_Y);
    EXPECT_MOVE(move_by(&p, 30, 300), UI_PRESS_MOVED, false, 1, 1, 0);

    ui_pager_press(&p, START_X, START_Y);
    EXPECT_MOVE(move_by(&p, -200, -201), UI_PRESS_MOVED, false, 1, 1, 0);
    TEST_ASSERT_FALSE(ui_pager_release(&p));
    TEST_ASSERT_EQUAL_UINT8(1, p.index);
}

TEST_CASE("the rest position is minus index times page width", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 0);
    TEST_ASSERT_EQUAL_INT16(0, ui_pager_rest_x(&p));

    EXPECT_MOVE(swipe(&p, -200), UI_PRESS_SWIPED, true, 0, 1, 0);
    TEST_ASSERT_EQUAL_INT16(-1024, ui_pager_rest_x(&p));
    ui_pager_release(&p);

    TEST_ASSERT_TRUE(ui_pager_goto(&p, 2));
    TEST_ASSERT_EQUAL_INT16(-2048, ui_pager_rest_x(&p));
    TEST_ASSERT_TRUE(ui_pager_goto(&p, 0));
    TEST_ASSERT_EQUAL_INT16(0, ui_pager_rest_x(&p));
}

TEST_CASE("a press in progress does not move the rest position", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 1);
    ui_pager_press(&p, START_X, START_Y);
    TEST_ASSERT_EQUAL_INT16(-1024, ui_pager_rest_x(&p));
    begin_drag(&p, -1);
    EXPECT_MOVE(move_by(&p, -60, 0), UI_PRESS_DRAG, false, 1, 1, 0);
    TEST_ASSERT_EQUAL_INT16(-1024, ui_pager_rest_x(&p));
}

TEST_CASE("goto jumps to a valid page", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 0);
    TEST_ASSERT_TRUE(ui_pager_goto(&p, 2));
    TEST_ASSERT_EQUAL_UINT8(2, p.index);
    TEST_ASSERT_TRUE_MESSAGE(ui_pager_goto(&p, 2), "going to the page already showing is fine");
    TEST_ASSERT_TRUE(ui_pager_goto(&p, 0));
    TEST_ASSERT_EQUAL_UINT8(0, p.index);
}

TEST_CASE("goto an index at or past count is refused and changes nothing", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 1);
    TEST_ASSERT_FALSE(ui_pager_goto(&p, 3));
    TEST_ASSERT_FALSE(ui_pager_goto(&p, 255));
    TEST_ASSERT_EQUAL_UINT8(1, p.index);
    TEST_ASSERT_EQUAL_INT16(-1024, ui_pager_rest_x(&p));

    begin_drag(&p, -1);
    TEST_ASSERT_FALSE(ui_pager_goto(&p, 3));
    TEST_ASSERT_EQUAL_MESSAGE(UI_PRESS_DRAG, p.press, "a refused goto must not end the press");
    EXPECT_MOVE(move_by(&p, -96, 0), UI_PRESS_SWIPED, true, 1, 2, 0);
}

TEST_CASE("goto ends a press in progress", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 1);
    begin_drag(&p, -1);
    TEST_ASSERT_TRUE(ui_pager_goto(&p, 0));
    TEST_ASSERT_EQUAL(UI_PRESS_NONE, p.press);
    EXPECT_MOVE(move_by(&p, -200, 0), UI_PRESS_NONE, false, 0, 0, 0);
    TEST_ASSERT_FALSE(ui_pager_release(&p));
    TEST_ASSERT_EQUAL_UINT8(0, p.index);

    ui_pager_press(&p, START_X, START_Y);
    TEST_ASSERT_TRUE(ui_pager_goto(&p, 2));
    TEST_ASSERT_FALSE_MESSAGE(ui_pager_release(&p), "a press ended by goto is not a tap");
}

TEST_CASE("a release without a press is not a tap and changes nothing", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 2);
    TEST_ASSERT_FALSE(ui_pager_release(&p));
    TEST_ASSERT_EQUAL_UINT8(2, p.index);

    ui_pager_press(&p, START_X, START_Y);
    TEST_ASSERT_TRUE(ui_pager_release(&p));
    TEST_ASSERT_FALSE_MESSAGE(ui_pager_release(&p), "the press ended with the first release");
    TEST_ASSERT_EQUAL_UINT8(2, p.index);
}

TEST_CASE("a move without a press does nothing", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 1);
    EXPECT_MOVE(move_by(&p, -200, 0), UI_PRESS_NONE, false, 1, 1, 0);
    EXPECT_MOVE(move_by(&p, 200, 200), UI_PRESS_NONE, false, 1, 1, 0);

    ui_pager_press(&p, START_X, START_Y);
    ui_pager_release(&p);
    EXPECT_MOVE(move_by(&p, -200, 0), UI_PRESS_NONE, false, 1, 1, 0);
    TEST_ASSERT_EQUAL_UINT8(1, p.index);
}

TEST_CASE("a second press replaces the one in progress", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(3, 1);
    begin_drag(&p, -1);
    EXPECT_MOVE(move_by(&p, -60, 0), UI_PRESS_DRAG, false, 1, 1, 0);

    ui_pager_press(&p, 800, 100);
    TEST_ASSERT_EQUAL(UI_PRESS_TAP, p.press);
    EXPECT_MOVE(ui_pager_move(&p, 810, 100), UI_PRESS_TAP, false, 1, 1, 0);
    TEST_ASSERT_TRUE(ui_pager_release(&p));

    ui_pager_press(&p, 500, 300);
    EXPECT_MOVE(ui_pager_move(&p, 500, 330), UI_PRESS_MOVED, false, 1, 1, 0);
    ui_pager_press(&p, 500, 300);
    EXPECT_MOVE(ui_pager_move(&p, 470, 300), UI_PRESS_DRAG, false, 1, 1, 0);

    ui_pager_press(&p, 800, 100);
    EXPECT_MOVE(ui_pager_move(&p, 704, 100), UI_PRESS_SWIPED, true, 1, 2, 0);
}

TEST_CASE("a count of 0 behaves as a single page", "[ui_pager]")
{
    ui_pager_t p;
    ui_pager_init(&p, &k_cfg, 0);
    TEST_ASSERT_EQUAL_UINT8(1, p.count);

    EXPECT_MOVE(swipe(&p, -200), UI_PRESS_SWIPED, false, 0, 0, -1);
    EXPECT_MOVE(swipe(&p, 200), UI_PRESS_SWIPED, false, 0, 0, 1);
    TEST_ASSERT_EQUAL_UINT8(0, p.index);

    TEST_ASSERT_TRUE(ui_pager_goto(&p, 0));
    TEST_ASSERT_FALSE(ui_pager_goto(&p, 1));
}

/* On two pages a leftward swipe is only possible from page 0 and a rightward one from page 1, so swipe i
 * (counting from 1) is leftward when i is odd. */
static void alternate_swipes(ui_pager_t *p, int n)
{
    for (int i = 1; i <= n; i++) {
        const int odd = i % 2;
        EXPECT_MOVE(swipe(p, odd ? -200 : 200), UI_PRESS_SWIPED, true, 1 - odd, odd, 0);
        TEST_ASSERT_FALSE(ui_pager_release(p));
    }
}

TEST_CASE("100 alternating swipes on two pages end on page 0", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(2, 0);
    alternate_swipes(&p, 100);
    TEST_ASSERT_EQUAL_UINT8(0, p.index);
    TEST_ASSERT_EQUAL_INT16(0, ui_pager_rest_x(&p));
}

TEST_CASE("101 alternating swipes on two pages end on page 1", "[ui_pager]")
{
    ui_pager_t p = pager_on_page(2, 0);
    alternate_swipes(&p, 101);
    TEST_ASSERT_EQUAL_UINT8(1, p.index);
    TEST_ASSERT_EQUAL_INT16(-1024, ui_pager_rest_x(&p));
}
