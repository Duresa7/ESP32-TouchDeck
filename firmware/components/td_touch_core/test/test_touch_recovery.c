#include <stdbool.h>
#include <stdint.h>
#include "unity.h"
#include "touch_recovery.h"

#define STEP_MS 10

static td_touch_sample_t press(uint16_t x, uint16_t y)
{
    return (td_touch_sample_t){ .x = x, .y = y, .pressed = true };
}

static td_touch_sample_t lift(uint16_t x, uint16_t y)
{
    return (td_touch_sample_t){ .x = x, .y = y, .pressed = false };
}

/* Presses at (x, y) every STEP_MS from `from_ms` to `to_ms` inclusive and returns the first step that armed, or the
 * last step when none did. */
static td_recovery_step_t hold(td_recovery_watch_t *w, uint16_t x, uint16_t y, uint32_t from_ms, uint32_t to_ms)
{
    td_recovery_step_t step = { 0 };
    for (uint32_t t = from_ms; t <= to_ms; t += STEP_MS) {
        step = td_recovery_watch_step(w, press(x, y), t);
        if (step.armed) {
            break;
        }
    }
    return step;
}

TEST_CASE("a hold of 9999 ms does not arm", "[touch_recovery]")
{
    td_recovery_watch_t w = { 0 };
    hold(&w, 512, 300, 0, 9990);
    const td_recovery_step_t step = td_recovery_watch_step(&w, press(512, 300), 9999);
    TEST_ASSERT_FALSE(step.armed);
    TEST_ASSERT_FALSE(step.swallow);
    TEST_ASSERT_EQUAL(TD_RECOVERY_HOLDING, w.state);
    TEST_ASSERT_EQUAL_UINT32(9999, w.held_ms);
    TEST_ASSERT_EQUAL_UINT32(0, w.armed_count);
}

TEST_CASE("a hold of 10000 ms arms once", "[touch_recovery]")
{
    td_recovery_watch_t w = { 0 };
    const td_recovery_step_t step = hold(&w, 512, 300, 0, 10000);
    TEST_ASSERT_TRUE(step.armed);
    TEST_ASSERT_TRUE(step.swallow);
    TEST_ASSERT_FALSE(step.released);
    TEST_ASSERT_EQUAL(TD_RECOVERY_ARMED, w.state);
    TEST_ASSERT_EQUAL_UINT32(1, w.armed_count);

    const td_recovery_step_t later = td_recovery_watch_step(&w, press(512, 300), 10010);
    TEST_ASSERT_FALSE(later.armed);
    TEST_ASSERT_TRUE(later.swallow);
    TEST_ASSERT_EQUAL_UINT32(1, w.armed_count);
}

TEST_CASE("a hold within the slop still arms", "[touch_recovery]")
{
    td_recovery_watch_t w = { 0 };
    hold(&w, 500, 300, 0, 5000);
    hold(&w, 500 + TD_RECOVERY_SLOP_PX, 300, 5010, 9990);
    const td_recovery_step_t step = td_recovery_watch_step(&w, press(500, 300 - TD_RECOVERY_SLOP_PX), 10000);
    TEST_ASSERT_TRUE(step.armed);
}

TEST_CASE("a move of 25 px at 9 seconds resets the hold", "[touch_recovery]")
{
    td_recovery_watch_t w = { 0 };
    hold(&w, 500, 300, 0, 8990);
    const td_recovery_step_t move = td_recovery_watch_step(&w, press(500 + TD_RECOVERY_SLOP_PX + 1, 300), 9000);
    TEST_ASSERT_FALSE(move.armed);
    TEST_ASSERT_EQUAL(TD_RECOVERY_IDLE, w.state);

    /* The same contact, back at its first point and held past 10 seconds in all, never arms. */
    const td_recovery_step_t after = hold(&w, 500, 300, 9010, 20000);
    TEST_ASSERT_FALSE(after.armed);
    TEST_ASSERT_FALSE(after.swallow);
    TEST_ASSERT_EQUAL_UINT32(0, w.armed_count);
}

TEST_CASE("a diagonal move past the slop resets the hold", "[touch_recovery]")
{
    td_recovery_watch_t w = { 0 };
    hold(&w, 500, 300, 0, 1000);
    /* 18 px on each axis is 25.5 px away, though neither axis alone passes 24. */
    td_recovery_watch_step(&w, press(518, 318), 1010);
    TEST_ASSERT_EQUAL(TD_RECOVERY_IDLE, w.state);
}

TEST_CASE("a release and a new hold start a new count", "[touch_recovery]")
{
    td_recovery_watch_t w = { 0 };
    hold(&w, 512, 300, 0, 9000);
    td_recovery_step_t step = td_recovery_watch_step(&w, lift(512, 300), 9010);
    TEST_ASSERT_FALSE(step.swallow);
    TEST_ASSERT_FALSE(step.released);
    TEST_ASSERT_EQUAL(TD_RECOVERY_IDLE, w.state);

    /* 9 more seconds from a new press is under the limit, and the old 9 seconds do not count. */
    step = hold(&w, 100, 100, 9020, 18010);
    TEST_ASSERT_FALSE(step.armed);
    step = hold(&w, 100, 100, 18020, 19020);
    TEST_ASSERT_TRUE(step.armed);
    TEST_ASSERT_EQUAL_UINT32(1, w.armed_count);
}

TEST_CASE("every sample of an armed contact is swallowed until its release", "[touch_recovery]")
{
    td_recovery_watch_t w = { 0 };
    TEST_ASSERT_TRUE(hold(&w, 512, 300, 0, 10000).armed);

    /* A move past the slop after arming changes nothing. */
    td_recovery_step_t step = td_recovery_watch_step(&w, press(900, 500), 10020);
    TEST_ASSERT_TRUE(step.swallow);
    TEST_ASSERT_FALSE(step.armed);
    TEST_ASSERT_EQUAL(TD_RECOVERY_ARMED, w.state);

    step = td_recovery_watch_step(&w, lift(900, 500), 10030);
    TEST_ASSERT_TRUE(step.swallow);
    TEST_ASSERT_TRUE(step.released);
    TEST_ASSERT_EQUAL(TD_RECOVERY_IDLE, w.state);

    /* The next contact is a normal one again. */
    step = td_recovery_watch_step(&w, press(10, 10), 10040);
    TEST_ASSERT_FALSE(step.swallow);
    TEST_ASSERT_EQUAL(TD_RECOVERY_HOLDING, w.state);
    TEST_ASSERT_EQUAL_UINT32(1, w.armed_count);
}

TEST_CASE("a hold on a dark screen arms by time, however the reads are spaced", "[touch_recovery]")
{
    /* The first sample of a contact on a dark screen reaches the watcher, and the read that wakes the panel runs late
     * and the reads after it come slowly, one per frame. Only the clock counts. */
    td_recovery_watch_t w = { 0 };
    uint32_t t = 0;
    uint32_t samples = 0;
    td_recovery_step_t step = td_recovery_watch_step(&w, press(512, 300), t);
    t += 400;
    while (!step.armed && t < 20000) {
        step = td_recovery_watch_step(&w, press(512, 300), t);
        samples++;
        TEST_ASSERT_TRUE(t < TD_RECOVERY_HOLD_MS || step.armed);
        t += 65;
    }
    TEST_ASSERT_TRUE(step.armed);
    TEST_ASSERT_TRUE(samples < 200);
    TEST_ASSERT_TRUE(w.held_ms >= TD_RECOVERY_HOLD_MS);
    TEST_ASSERT_TRUE(w.held_ms < TD_RECOVERY_HOLD_MS + 65);
}

TEST_CASE("a clock that wraps mid-hold still arms", "[touch_recovery]")
{
    td_recovery_watch_t w = { 0 };
    const uint32_t start = UINT32_MAX - 4990;
    td_recovery_step_t step = { 0 };
    for (uint32_t i = 0; i <= 1000 && !step.armed; i++) {
        step = td_recovery_watch_step(&w, press(512, 300), start + i * STEP_MS);
    }
    TEST_ASSERT_TRUE(step.armed);
}

TEST_CASE("a lift with no hold in progress says nothing", "[touch_recovery]")
{
    td_recovery_watch_t w = { 0 };
    const td_recovery_step_t step = td_recovery_watch_step(&w, lift(0, 0), 0);
    TEST_ASSERT_FALSE(step.swallow);
    TEST_ASSERT_FALSE(step.armed);
    TEST_ASSERT_FALSE(step.released);
    TEST_ASSERT_EQUAL(TD_RECOVERY_IDLE, w.state);
}

TEST_CASE("the state names are the ones the console reports", "[touch_recovery]")
{
    TEST_ASSERT_EQUAL_STRING("IDLE", td_recovery_state_name(TD_RECOVERY_IDLE));
    TEST_ASSERT_EQUAL_STRING("HOLDING", td_recovery_state_name(TD_RECOVERY_HOLDING));
    TEST_ASSERT_EQUAL_STRING("ARMED", td_recovery_state_name(TD_RECOVERY_ARMED));
}
