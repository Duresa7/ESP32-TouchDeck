#include "key_sink_fanout.h"
#include "unity.h"

typedef struct {
    int order[8];
    unsigned count;
    uint8_t usage[8];
} seen_t;

static int s_clock;

static void record(void *ctx, const key_report_t *report)
{
    seen_t *seen = ctx;
    seen->order[seen->count] = s_clock++;
    seen->usage[seen->count] = report->kbd.keys[0];
    seen->count++;
}

TEST_CASE("three sinks each see every report in order", "[key_sink_fanout]")
{
    seen_t a = { 0 };
    seen_t b = { 0 };
    seen_t c = { 0 };
    key_sink_fanout_t fanout = { .sinks = { { record, &a }, { record, &b }, { record, &c } } };
    const key_sink_t sink = key_sink_fanout_sink(&fanout);
    s_clock = 0;
    for (uint8_t usage = 1; usage <= 3; usage++) {
        const key_report_t report = key_report_kbd(0, usage);
        sink.report(sink.ctx, &report);
    }
    seen_t *all[] = { &a, &b, &c };
    for (unsigned i = 0; i < 3; i++) {
        TEST_ASSERT_EQUAL_UINT(3, all[i]->count);
        for (unsigned r = 0; r < 3; r++) {
            TEST_ASSERT_EQUAL_UINT8(r + 1, all[i]->usage[r]);
        }
    }
    /* Report 1 reaches sink a, b, c before report 2 reaches any. */
    TEST_ASSERT_EQUAL_INT(0, a.order[0]);
    TEST_ASSERT_EQUAL_INT(1, b.order[0]);
    TEST_ASSERT_EQUAL_INT(2, c.order[0]);
    TEST_ASSERT_EQUAL_INT(3, a.order[1]);
}

TEST_CASE("a NULL slot is skipped", "[key_sink_fanout]")
{
    seen_t a = { 0 };
    seen_t c = { 0 };
    key_sink_fanout_t fanout = { .sinks = { { record, &a }, { NULL, NULL }, { record, &c } } };
    const key_sink_t sink = key_sink_fanout_sink(&fanout);
    const key_report_t report = key_report_kbd(1, 62);
    sink.report(sink.ctx, &report);
    TEST_ASSERT_EQUAL_UINT(1, a.count);
    TEST_ASSERT_EQUAL_UINT(1, c.count);
    TEST_ASSERT_EQUAL_UINT8(62, c.usage[0]);
}
