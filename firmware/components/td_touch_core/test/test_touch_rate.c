#include <stdint.h>
#include "unity.h"
#include "touch_rate.h"

static void press_frames(td_touch_rate_t *r, uint32_t n, int64_t start_us, int64_t period_us)
{
    for (uint32_t i = 0; i < n; i++) {
        td_touch_rate_frame(r, true, start_us + (int64_t)i * period_us);
    }
}

static void five_reports_then_release(td_touch_rate_t *r)
{
    press_frames(r, 5, 1000000, 10000);
    td_touch_rate_frame(r, false, 1090000);
}

TEST_CASE("five pressed reports 10 ms apart read as 100 hz over 40 ms", "[touch_rate]")
{
    td_touch_rate_t r = { 0 };
    press_frames(&r, 5, 1000000, 10000);

    TEST_ASSERT_EQUAL_UINT32(5, r.frames);
    TEST_ASSERT_TRUE_MESSAGE(r.down, "the finger is still down");
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(40, td_touch_rate_span_ms(&r), "4 intervals of 10 ms span 40 ms");
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(100, td_touch_rate_hz(&r), "4 intervals in 40000 us: 4 * 1e6 / 40000 = 100");
}

TEST_CASE("a release keeps the figures of the contact it ends", "[touch_rate]")
{
    td_touch_rate_t r = { 0 };
    five_reports_then_release(&r);

    TEST_ASSERT_FALSE_MESSAGE(r.down, "the release ends the contact");
    TEST_ASSERT_EQUAL_UINT32(5, r.frames);
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(40, td_touch_rate_span_ms(&r), "the release must not stretch the span to 90 ms");
    TEST_ASSERT_EQUAL_UINT32(100, td_touch_rate_hz(&r));

    td_touch_rate_frame(&r, false, 1200000);
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(40, td_touch_rate_span_ms(&r), "a second release changes nothing");
    TEST_ASSERT_EQUAL_UINT32(100, td_touch_rate_hz(&r));
}

TEST_CASE("the next pressed report after a release starts a fresh contact", "[touch_rate]")
{
    td_touch_rate_t r = { 0 };
    five_reports_then_release(&r);

    td_touch_rate_frame(&r, true, 2000000);
    TEST_ASSERT_TRUE(r.down);
    TEST_ASSERT_EQUAL_UINT32(1, r.frames);
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(0, td_touch_rate_hz(&r), "one report has no rate, whatever came before");
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(0, td_touch_rate_span_ms(&r), "the span restarts at the new first report");

    td_touch_rate_frame(&r, true, 2010000);
    TEST_ASSERT_EQUAL_UINT32(2, r.frames);
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(10, td_touch_rate_span_ms(&r), "1 interval of 10 ms");
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(100, td_touch_rate_hz(&r), "1 interval in 10000 us: 1 * 1e6 / 10000 = 100");
}

TEST_CASE("a single pressed report has no rate yet", "[touch_rate]")
{
    td_touch_rate_t r = { 0 };
    td_touch_rate_frame(&r, true, 5000000);

    TEST_ASSERT_EQUAL_UINT32(1, r.frames);
    TEST_ASSERT_EQUAL_UINT32(0, td_touch_rate_hz(&r));
    TEST_ASSERT_EQUAL_UINT32(0, td_touch_rate_span_ms(&r));
}

TEST_CASE("a zeroed struct has seen nothing", "[touch_rate]")
{
    const td_touch_rate_t r = { 0 };

    TEST_ASSERT_EQUAL_UINT32(0, td_touch_rate_hz(&r));
    TEST_ASSERT_EQUAL_UINT32(0, td_touch_rate_span_ms(&r));
}

TEST_CASE("two reports stamped with the same time have no rate", "[touch_rate]")
{
    td_touch_rate_t r = { 0 };
    td_touch_rate_frame(&r, true, 3000000);
    td_touch_rate_frame(&r, true, 3000000);

    TEST_ASSERT_EQUAL_UINT32(2, r.frames);
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(0, td_touch_rate_hz(&r), "a zero span must report 0, not divide by it");
    TEST_ASSERT_EQUAL_UINT32(0, td_touch_rate_span_ms(&r));
}

TEST_CASE("hz rounds to the nearest whole report per second", "[touch_rate]")
{
    td_touch_rate_t up = { 0 };
    press_frames(&up, 5, 1000000, 7000);
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(143, td_touch_rate_hz(&up),
                                     "4 intervals in 28000 us: 4 * 1e6 / 28000 = 142.857, nearest is 143");

    td_touch_rate_t down = { 0 };
    press_frames(&down, 5, 1000000, 9000);
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(111, td_touch_rate_hz(&down),
                                     "4 intervals in 36000 us: 4 * 1e6 / 36000 = 111.111, nearest is 111");
}

TEST_CASE("hz is the interval count over the whole contact however the reports are spaced", "[touch_rate]")
{
    td_touch_rate_t r = { 0 };
    td_touch_rate_frame(&r, true, 7000000);
    td_touch_rate_frame(&r, true, 7005000);
    td_touch_rate_frame(&r, true, 7040000);

    TEST_ASSERT_EQUAL_UINT32(40, td_touch_rate_span_ms(&r));
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(50, td_touch_rate_hz(&r), "2 intervals in 40000 us: 2 * 1e6 / 40000 = 50");
}

TEST_CASE("a minute-long contact at 100 hz still reads 100 hz", "[touch_rate]")
{
    td_touch_rate_t r = { 0 };
    press_frames(&r, 6001, 5000000000LL, 10000);

    TEST_ASSERT_EQUAL_UINT32(6001, r.frames);
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(60000, td_touch_rate_span_ms(&r), "6000 intervals of 10 ms span 60000 ms");
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(100, td_touch_rate_hz(&r),
                                     "6000 intervals in 60000000 us: 6000 * 1e6 / 60000000 = 100, past 32-bit range");
}
