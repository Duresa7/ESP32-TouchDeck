#include <string.h>
#include "unity.h"
#include "key_sink_log.h"

static key_sink_log_t s_log;
static key_log_entry_t s_entries[KEY_LOG_RECORD_CAP];
static uint32_t s_now;
static char s_printed[KEY_LOG_LINE_MAX];
static unsigned s_prints;

static uint32_t fake_now(void)
{
    return s_now;
}

static void fake_print(const char *line)
{
    strlcpy(s_printed, line, sizeof s_printed);
    s_prints++;
}

static void fresh_log(void)
{
    key_sink_log_init(&s_log, s_entries, KEY_LOG_RECORD_CAP, fake_now, fake_print);
    s_now = 0;
    s_printed[0] = '\0';
    s_prints = 0;
}

static void take(const key_report_t r)
{
    const key_sink_t sink = key_sink_log_sink(&s_log);
    sink.report(sink.ctx, &r);
}

static void expect_format(const key_log_entry_t *e, const char *want)
{
    char buf[KEY_LOG_LINE_MAX];
    TEST_ASSERT_EQUAL_UINT32(strlen(want), key_sink_log_format(e, buf, sizeof buf));
    TEST_ASSERT_EQUAL_STRING(want, buf);
}

TEST_CASE("the log line of each report type", "[key_sink_log]")
{
    key_log_entry_t e = { .seq = 7, .t = 1234, .report = key_report_kbd(2, 106) };
    expect_format(&e, "{\"seq\":7,\"t\":1234,\"type\":\"kbd\",\"mods\":2,\"keys\":[106]}");

    e.report = key_report_kbd(0, 0);
    expect_format(&e, "{\"seq\":7,\"t\":1234,\"type\":\"kbd\",\"mods\":0,\"keys\":[]}");

    e.report = key_report_con(205);
    expect_format(&e, "{\"seq\":7,\"t\":1234,\"type\":\"con\",\"usage\":205}");

    e.report = key_report_con(0);
    expect_format(&e, "{\"seq\":7,\"t\":1234,\"type\":\"con\",\"usage\":0}");

    e.report = key_report_kbd(15, 58);
    e.report.kbd.keys[1] = 69;
    e.report.kbd.keys[5] = 115;
    expect_format(&e, "{\"seq\":7,\"t\":1234,\"type\":\"kbd\",\"mods\":15,\"keys\":[58,69,115]}");

    e.seq = UINT32_MAX;
    e.t = UINT32_MAX;
    e.report = key_report_kbd(255, 255);
    for (unsigned i = 1; i < KEY_REPORT_KEYS; i++) {
        e.report.kbd.keys[i] = 255;
    }
    expect_format(&e, "{\"seq\":4294967295,\"t\":4294967295,\"type\":\"kbd\",\"mods\":255,\"keys\":[255,255,255,255,255,255]}");
    TEST_ASSERT_TRUE(strlen("{\"seq\":4294967295,\"t\":4294967295,\"type\":\"kbd\",\"mods\":255,\"keys\":[255,255,255,255,255,255]}") < KEY_LOG_LINE_MAX);

    /* A short buffer gets a cut line and the full length, and nothing past its end. */
    char small[24];
    memset(small, 'x', sizeof small);
    TEST_ASSERT_EQUAL_UINT32(90, key_sink_log_format(&e, small, 20));
    TEST_ASSERT_EQUAL_UINT32(19, strlen(small));
    TEST_ASSERT_EQUAL_MEMORY("{\"seq\":4294967295,\"", small, 19);
    for (unsigned i = 20; i < sizeof small; i++) {
        TEST_ASSERT_EQUAL_CHAR('x', small[i]);
    }
}

TEST_CASE("with the log on each report prints once with seq and t, with it off nothing prints", "[key_sink_log]")
{
    fresh_log();
    s_now = 500;
    take(key_report_kbd(2, 106));
    TEST_ASSERT_EQUAL_UINT32(0, s_prints);
    TEST_ASSERT_EQUAL_UINT32(1, key_sink_log_seq(&s_log));

    key_sink_log_set_log(&s_log, true);
    TEST_ASSERT_TRUE(key_sink_log_log_on(&s_log));
    s_now = 501;
    take(key_report_kbd(0, 0));
    TEST_ASSERT_EQUAL_UINT32(1, s_prints);
    TEST_ASSERT_EQUAL_STRING("{\"seq\":2,\"t\":501,\"type\":\"kbd\",\"mods\":0,\"keys\":[]}", s_printed);
    s_now = 777;
    take(key_report_con(233));
    TEST_ASSERT_EQUAL_UINT32(2, s_prints);
    TEST_ASSERT_EQUAL_STRING("{\"seq\":3,\"t\":777,\"type\":\"con\",\"usage\":233}", s_printed);

    key_sink_log_set_log(&s_log, false);
    take(key_report_con(0));
    TEST_ASSERT_EQUAL_UINT32(2, s_prints);
    TEST_ASSERT_EQUAL_UINT32(4, key_sink_log_seq(&s_log));
    TEST_ASSERT_EQUAL_UINT32(0, key_sink_log_count(&s_log));
}

TEST_CASE("a record of 2048 reports, and the 2049th counted in dropped", "[key_sink_log]")
{
    fresh_log();
    take(key_report_kbd(0, 58));
    TEST_ASSERT_EQUAL_UINT32(0, key_sink_log_count(&s_log));

    key_sink_log_record_start(&s_log);
    TEST_ASSERT_TRUE(key_sink_log_recording(&s_log));
    for (unsigned i = 0; i < KEY_LOG_RECORD_CAP; i++) {
        s_now = 1000 + i;
        take(i % 2 == 0 ? key_report_kbd(2, 109) : key_report_kbd(0, 0));
    }
    TEST_ASSERT_EQUAL_UINT32(KEY_LOG_RECORD_CAP, key_sink_log_count(&s_log));
    TEST_ASSERT_EQUAL_UINT32(0, key_sink_log_dropped(&s_log));

    take(key_report_kbd(2, 109));
    TEST_ASSERT_EQUAL_UINT32(KEY_LOG_RECORD_CAP, key_sink_log_count(&s_log));
    TEST_ASSERT_EQUAL_UINT32(1, key_sink_log_dropped(&s_log));
    take(key_report_kbd(0, 0));
    TEST_ASSERT_EQUAL_UINT32(2, key_sink_log_dropped(&s_log));

    const key_log_entry_t *first = key_sink_log_entry(&s_log, 0);
    TEST_ASSERT_EQUAL_UINT32(2, first->seq);
    TEST_ASSERT_EQUAL_UINT32(1000, first->t);
    TEST_ASSERT_EQUAL_UINT8(109, first->report.kbd.keys[0]);
    const key_log_entry_t *last = key_sink_log_entry(&s_log, KEY_LOG_RECORD_CAP - 1);
    TEST_ASSERT_EQUAL_UINT32(KEY_LOG_RECORD_CAP + 1, last->seq);
    TEST_ASSERT_EQUAL_UINT32(1000 + KEY_LOG_RECORD_CAP - 1, last->t);
    TEST_ASSERT_TRUE(key_report_is_release(&last->report));
    for (unsigned i = 1; i < KEY_LOG_RECORD_CAP; i++) {
        TEST_ASSERT_EQUAL_UINT32(key_sink_log_entry(&s_log, i - 1)->seq + 1, key_sink_log_entry(&s_log, i)->seq);
    }

    key_sink_log_record_stop(&s_log);
    TEST_ASSERT_FALSE(key_sink_log_recording(&s_log));
    take(key_report_con(205));
    TEST_ASSERT_EQUAL_UINT32(2, key_sink_log_dropped(&s_log));
    TEST_ASSERT_EQUAL_UINT32(KEY_LOG_RECORD_CAP, key_sink_log_count(&s_log));
    TEST_ASSERT_EQUAL_UINT32(KEY_LOG_RECORD_CAP + 4, key_sink_log_seq(&s_log));
}

TEST_CASE("record start empties the record and keeps counting seq since boot", "[key_sink_log]")
{
    fresh_log();
    key_sink_log_record_start(&s_log);
    take(key_report_kbd(0, 58));
    take(key_report_kbd(0, 0));
    TEST_ASSERT_EQUAL_UINT32(2, key_sink_log_count(&s_log));

    key_sink_log_record_start(&s_log);
    TEST_ASSERT_EQUAL_UINT32(0, key_sink_log_count(&s_log));
    TEST_ASSERT_EQUAL_UINT32(0, key_sink_log_dropped(&s_log));
    take(key_report_con(226));
    TEST_ASSERT_EQUAL_UINT32(1, key_sink_log_count(&s_log));
    TEST_ASSERT_EQUAL_UINT32(3, key_sink_log_entry(&s_log, 0)->seq);
}

TEST_CASE("a sink with no record logs and counts every report as dropped while recording", "[key_sink_log]")
{
    key_sink_log_init(&s_log, NULL, 99, fake_now, fake_print);
    s_prints = 0;
    key_sink_log_set_log(&s_log, true);
    key_sink_log_record_start(&s_log);
    take(key_report_kbd(0, 58));
    TEST_ASSERT_EQUAL_UINT32(1, s_prints);
    TEST_ASSERT_EQUAL_UINT32(0, key_sink_log_count(&s_log));
    TEST_ASSERT_EQUAL_UINT32(1, key_sink_log_dropped(&s_log));
}
