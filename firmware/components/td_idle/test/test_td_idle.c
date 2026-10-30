#include <stdbool.h>
#include <stdint.h>
#include "unity.h"
#include "td_idle.h"

#define T0 INT64_C(1000000000)
#define MS INT64_C(1000)
#define SEC INT64_C(1000000)
#define LIMIT TD_PHOTO_BYTES_MAX

TEST_CASE("idle settings default to off, 60 s and 15 s and keep to the protocol's ranges", "[idle]")
{
    td_idle_settings_t s;
    td_idle_defaults(&s);
    TEST_ASSERT_FALSE(s.enabled);
    TEST_ASSERT_EQUAL_UINT32(60, s.timeout_s);
    TEST_ASSERT_EQUAL_UINT32(15, s.interval_s);
    TEST_ASSERT_TRUE(td_idle_valid(&s));

    const td_idle_settings_t edges[] = {
        { true, 10, 5 }, { true, 86400, 3600 }, { false, 10, 3600 },
    };
    for (size_t i = 0; i < sizeof edges / sizeof edges[0]; i++) {
        TEST_ASSERT_TRUE(td_idle_valid(&edges[i]));
    }
    const td_idle_settings_t outside[] = {
        { true, 9, 15 }, { true, 86401, 15 }, { true, 60, 4 }, { true, 60, 3601 }, { false, 0, 0 },
    };
    for (size_t i = 0; i < sizeof outside / sizeof outside[0]; i++) {
        TEST_ASSERT_FALSE(td_idle_valid(&outside[i]));
    }
}

TEST_CASE("a picture is 1 byte to the limit, and never over 128 KiB whatever the limit says", "[idle]")
{
    const td_photo_pace_t pace = { 0 };
    uint32_t retry = 0;
    TEST_ASSERT_EQUAL(TD_PHOTO_SIZE, td_photo_admit(&pace, 0, LIMIT, true, 1, 15, T0, &retry));
    TEST_ASSERT_EQUAL(TD_PHOTO_OK, td_photo_admit(&pace, 1, LIMIT, true, 1, 15, T0, &retry));
    TEST_ASSERT_EQUAL(TD_PHOTO_OK, td_photo_admit(&pace, LIMIT, LIMIT, true, 1, 15, T0, &retry));
    TEST_ASSERT_EQUAL(TD_PHOTO_SIZE, td_photo_admit(&pace, LIMIT + 1, LIMIT, true, 1, 15, T0, &retry));
    TEST_ASSERT_EQUAL(TD_PHOTO_SIZE, td_photo_admit(&pace, 5000, 4096, true, 1, 15, T0, &retry));
    TEST_ASSERT_EQUAL(TD_PHOTO_SIZE, td_photo_admit(&pace, LIMIT + 1, UINT32_MAX, true, 1, 15, T0, &retry));
    TEST_ASSERT_EQUAL_MESSAGE(TD_PHOTO_SIZE, td_photo_admit(&pace, 0, LIMIT, false, 1, 15, T0, &retry),
                              "size is checked before idle");
}

TEST_CASE("an awake Device or an off photo frame refuses a picture as idle", "[idle]")
{
    const td_photo_pace_t pace = { 0 };
    uint32_t retry = 0;
    TEST_ASSERT_EQUAL(TD_PHOTO_IDLE, td_photo_admit(&pace, 1000, LIMIT, false, 0, 15, T0, &retry));
    TEST_ASSERT_EQUAL(TD_PHOTO_IDLE, td_photo_admit(&pace, 1000, LIMIT, false, 3, 15, T0, &retry));
}

TEST_CASE("the first picture of an idle period comes at once and the next waits the interval with retry_ms", "[idle]")
{
    td_photo_pace_t pace = { 0 };
    uint32_t retry = 0;
    TEST_ASSERT_EQUAL(TD_PHOTO_OK, td_photo_admit(&pace, 1000, LIMIT, true, 1, 15, T0, &retry));
    td_photo_shown(&pace, 1, T0);

    TEST_ASSERT_EQUAL(TD_PHOTO_RATE, td_photo_admit(&pace, 1000, LIMIT, true, 1, 15, T0, &retry));
    TEST_ASSERT_EQUAL_UINT32(15000, retry);
    TEST_ASSERT_EQUAL(TD_PHOTO_RATE, td_photo_admit(&pace, 1000, LIMIT, true, 1, 15, T0 + 10 * SEC + 1, &retry));
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(5000, retry, "4999.999 ms rounds up");
    TEST_ASSERT_EQUAL(TD_PHOTO_RATE, td_photo_admit(&pace, 1000, LIMIT, true, 1, 15, T0 + 15 * SEC - 1, &retry));
    TEST_ASSERT_EQUAL_UINT32(1, retry);
    TEST_ASSERT_EQUAL(TD_PHOTO_OK, td_photo_admit(&pace, 1000, LIMIT, true, 1, 15, T0 + 15 * SEC, &retry));

    TEST_ASSERT_EQUAL(TD_PHOTO_RATE, td_photo_admit(&pace, 1000, LIMIT, true, 1, 3600, T0 + 15 * SEC, &retry));
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(3585000, retry, "a photo retry_ms may exceed 1000");
}

TEST_CASE("a new idle period lets its first picture through at once", "[idle]")
{
    td_photo_pace_t pace = { 0 };
    uint32_t retry = 0;
    td_photo_shown(&pace, 1, T0);
    TEST_ASSERT_EQUAL(TD_PHOTO_RATE, td_photo_admit(&pace, 1000, LIMIT, true, 1, 15, T0 + SEC, &retry));
    TEST_ASSERT_EQUAL(TD_PHOTO_OK, td_photo_admit(&pace, 1000, LIMIT, true, 2, 15, T0 + SEC, &retry));
    td_photo_shown(&pace, 2, T0 + SEC);
    TEST_ASSERT_EQUAL(TD_PHOTO_RATE, td_photo_admit(&pace, 1000, LIMIT, true, 2, 15, T0 + 2 * SEC, &retry));
    TEST_ASSERT_EQUAL_UINT32(14000, retry);
}

TEST_CASE("the idle face shows the picture until the link drops or no picture comes for two intervals", "[idle]")
{
    TEST_ASSERT_EQUAL(TD_FACE_CLOCK, td_idle_face(true, false, 0, 15, T0));
    TEST_ASSERT_EQUAL(TD_FACE_PICTURE, td_idle_face(true, true, T0, 15, T0));
    TEST_ASSERT_EQUAL(TD_FACE_PICTURE, td_idle_face(true, true, T0, 15, T0 + 30 * SEC - 1));
    TEST_ASSERT_EQUAL(TD_FACE_CLOCK, td_idle_face(true, true, T0, 15, T0 + 30 * SEC));
    TEST_ASSERT_EQUAL_MESSAGE(TD_FACE_CLOCK, td_idle_face(false, true, T0, 15, T0), "offline is the clock at once");
}

TEST_CASE("the clock reads --:-- until it is set, then the local time counting on from the set", "[idle]")
{
    td_clock_t clock = { 0 };
    char text[6];
    TEST_ASSERT_FALSE(td_clock_text(&clock, T0, text));
    TEST_ASSERT_EQUAL_STRING("--:--", text);

    /* 2023-11-14 22:13:20 UTC */
    td_clock_set(&clock, 1700000000, 0, T0);
    TEST_ASSERT_TRUE(td_clock_text(&clock, T0, text));
    TEST_ASSERT_EQUAL_STRING("22:13", text);
    TEST_ASSERT_TRUE(td_clock_text(&clock, T0 + 39 * SEC, text));
    TEST_ASSERT_EQUAL_STRING("22:13", text);
    TEST_ASSERT_TRUE(td_clock_text(&clock, T0 + 40 * SEC, text));
    TEST_ASSERT_EQUAL_STRING("22:14", text);
    TEST_ASSERT_TRUE(td_clock_text(&clock, T0 - SEC, text));
    TEST_ASSERT_EQUAL_STRING_MESSAGE("22:13", text, "a time before the set reads as the set");

    td_clock_set(&clock, 1700000000, 120, T0);
    TEST_ASSERT_TRUE(td_clock_text(&clock, T0, text));
    TEST_ASSERT_EQUAL_STRING_MESSAGE("00:13", text, "the offset crosses midnight");
    td_clock_set(&clock, 1700000000, -840, T0);
    TEST_ASSERT_TRUE(td_clock_text(&clock, T0, text));
    TEST_ASSERT_EQUAL_STRING("08:13", text);
    td_clock_set(&clock, 0, -60, T0);
    TEST_ASSERT_TRUE(td_clock_text(&clock, T0, text));
    TEST_ASSERT_EQUAL_STRING_MESSAGE("23:00", text, "before 1970 in local time still reads a time of day");
}

TEST_CASE("the clock takes 0 to 4102444799 s and offsets of -840 to 840 min", "[idle]")
{
    TEST_ASSERT_TRUE(td_clock_valid(0, 0));
    TEST_ASSERT_TRUE(td_clock_valid(4102444799ull, 840));
    TEST_ASSERT_TRUE(td_clock_valid(1700000000, -840));
    TEST_ASSERT_FALSE(td_clock_valid(4102444800ull, 0));
    TEST_ASSERT_FALSE(td_clock_valid(0, 841));
    TEST_ASSERT_FALSE(td_clock_valid(0, -841));
}
