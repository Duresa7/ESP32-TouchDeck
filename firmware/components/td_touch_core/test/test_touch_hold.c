#include <stdint.h>
#include "unity.h"
#include "touch_hold.h"

#define EXPECT_SAMPLE(got, want_x, want_y, want_pressed)  \
    do {                                                  \
        TEST_ASSERT_EQUAL_UINT16((want_x), (got).x);      \
        TEST_ASSERT_EQUAL_UINT16((want_y), (got).y);      \
        TEST_ASSERT_EQUAL((want_pressed), (got).pressed); \
    } while (0)

static const td_touch_sample_t k_nothing = { 0 };

static td_touch_held_t held_after_press(uint16_t x, uint16_t y)
{
    td_touch_held_t h = { 0 };
    td_touch_held_poll(&h, TD_TOUCH_POLL_REPORT, (td_touch_sample_t){ .x = x, .y = y, .pressed = true });
    return h;
}

static void poll_errors(td_touch_held_t *h, int n)
{
    for (int i = 0; i < n; i++) {
        td_touch_held_poll(h, TD_TOUCH_POLL_ERROR, k_nothing);
    }
}

TEST_CASE("a report with a finger replaces the held sample", "[touch_hold]")
{
    td_touch_held_t h = held_after_press(100, 200);
    td_touch_held_poll(&h, TD_TOUCH_POLL_REPORT, (td_touch_sample_t){ .x = 640, .y = 480, .pressed = true });
    EXPECT_SAMPLE(h.sample, 640, 480, true);
}

TEST_CASE("a report without a finger lifts the press where it ended", "[touch_hold]")
{
    td_touch_held_t h = held_after_press(321, 123);
    td_touch_held_poll(&h, TD_TOUCH_POLL_REPORT, (td_touch_sample_t){ .x = 0, .y = 0, .pressed = false });
    EXPECT_SAMPLE(h.sample, 321, 123, false);
}

TEST_CASE("an idle poll keeps the press and its point", "[touch_hold]")
{
    td_touch_held_t h = held_after_press(321, 123);
    for (int i = 0; i < 50; i++) {
        td_touch_held_poll(&h, TD_TOUCH_POLL_IDLE, k_nothing);
    }
    EXPECT_SAMPLE(h.sample, 321, 123, true);
}

TEST_CASE("a run of failed polls lifts the press only at the limit", "[touch_hold]")
{
    td_touch_held_t h = held_after_press(321, 123);
    poll_errors(&h, TD_TOUCH_MAX_ERROR_RUN - 1);
    EXPECT_SAMPLE(h.sample, 321, 123, true);

    poll_errors(&h, 1);
    EXPECT_SAMPLE(h.sample, 321, 123, false);
}

TEST_CASE("any other poll ends a run of failures", "[touch_hold]")
{
    td_touch_held_t h = held_after_press(321, 123);
    poll_errors(&h, TD_TOUCH_MAX_ERROR_RUN - 1);
    td_touch_held_poll(&h, TD_TOUCH_POLL_IDLE, k_nothing);
    poll_errors(&h, TD_TOUCH_MAX_ERROR_RUN - 1);
    EXPECT_SAMPLE(h.sample, 321, 123, true);
}

TEST_CASE("a finger that returns after a dead bus presses again", "[touch_hold]")
{
    td_touch_held_t h = held_after_press(321, 123);
    poll_errors(&h, 3 * TD_TOUCH_MAX_ERROR_RUN);
    EXPECT_SAMPLE(h.sample, 321, 123, false);

    td_touch_held_poll(&h, TD_TOUCH_POLL_REPORT, (td_touch_sample_t){ .x = 50, .y = 60, .pressed = true });
    EXPECT_SAMPLE(h.sample, 50, 60, true);

    poll_errors(&h, 1);
    EXPECT_SAMPLE(h.sample, 50, 60, true);
}

TEST_CASE("failed polls with no finger down change nothing", "[touch_hold]")
{
    td_touch_held_t h = { .sample = { .x = 7, .y = 8, .pressed = false } };
    poll_errors(&h, 2 * TD_TOUCH_MAX_ERROR_RUN);
    EXPECT_SAMPLE(h.sample, 7, 8, false);
}
