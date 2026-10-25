#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include "unity.h"
#include "touch_queue.h"

#define POISON 0xA5

#define EXPECT_SAMPLE(got, want_x, want_y, want_pressed)  \
    do {                                                  \
        TEST_ASSERT_EQUAL_UINT16((want_x), (got).x);      \
        TEST_ASSERT_EQUAL_UINT16((want_y), (got).y);      \
        TEST_ASSERT_EQUAL((want_pressed), (got).pressed); \
    } while (0)

static td_touch_queue_t s_queue;
static td_touch_sample_t s_samples[TD_TOUCH_QUEUE_CAP];

static td_touch_queue_t *fresh_queue(void)
{
    memset(&s_queue, 0, sizeof s_queue);
    return &s_queue;
}

static td_touch_sample_t numbered(uint16_t id)
{
    return (td_touch_sample_t){ .x = id, .y = (uint16_t)(id + 1000), .pressed = id % 2 == 0 };
}

static void expect_numbered(uint16_t id, td_touch_sample_t got)
{
    const td_touch_sample_t want = numbered(id);
    TEST_ASSERT_EQUAL_UINT16_MESSAGE(want.x, got.x, "samples came out of order");
    TEST_ASSERT_EQUAL_UINT16_MESSAGE(want.y, got.y, "a sample came out with a changed y");
    TEST_ASSERT_EQUAL_MESSAGE(want.pressed, got.pressed, "a sample came out with a changed pressed flag");
}

static void fill_ids(uint16_t first, size_t n)
{
    for (size_t i = 0; i < n; i++) {
        s_samples[i] = numbered((uint16_t)(first + i));
    }
}

static void push_ids(td_touch_queue_t *q, uint16_t first, size_t n)
{
    fill_ids(first, n);
    TEST_ASSERT_TRUE_MESSAGE(td_touch_queue_push(q, s_samples, n), "the queue refused samples it had room for");
}

static void pop_ids(td_touch_queue_t *q, uint16_t first, size_t n)
{
    for (size_t i = 0; i < n; i++) {
        td_touch_sample_t got;
        TEST_ASSERT_TRUE_MESSAGE(td_touch_queue_pop(q, &got), "the queue ran dry before every sample came out");
        expect_numbered((uint16_t)(first + i), got);
    }
}

static void advance_head(td_touch_queue_t *q, size_t n)
{
    push_ids(q, 0, n);
    pop_ids(q, 0, n);
}

static void poison_samples(void)
{
    memset(s_samples, POISON, sizeof s_samples);
}

static void expect_samples_untouched(const char *message)
{
    const uint8_t *bytes = (const uint8_t *)s_samples;
    size_t changed = 0;
    for (size_t i = 0; i < sizeof s_samples; i++) {
        changed += bytes[i] != POISON;
    }
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(0, changed, message);
}

TEST_CASE("an injected sample wins over the real one and the real one returns when the queue drains", "[touch_queue]")
{
    td_touch_queue_t *q = fresh_queue();
    const td_touch_sample_t tap[] = {
        { .x = 512, .y = 490, .pressed = true },
        { .x = 512, .y = 490, .pressed = false },
    };
    const td_touch_sample_t finger = { .x = 100, .y = 200, .pressed = true };
    TEST_ASSERT_TRUE(td_touch_queue_push(q, tap, 2));

    bool injected = false;
    td_touch_sample_t got = td_touch_next(q, finger, &injected);
    EXPECT_SAMPLE(got, 512, 490, true);
    TEST_ASSERT_TRUE_MESSAGE(injected, "the injected press must be flagged as injected");

    injected = false;
    got = td_touch_next(q, finger, &injected);
    EXPECT_SAMPLE(got, 512, 490, false);
    TEST_ASSERT_TRUE_MESSAGE(injected, "the injected release must be flagged as injected");

    injected = true;
    got = td_touch_next(q, finger, &injected);
    EXPECT_SAMPLE(got, 100, 200, true);
    TEST_ASSERT_FALSE_MESSAGE(injected, "the real sample must not be flagged as injected");
}

TEST_CASE("an injected tap lands on bare glass, where the real sample is released", "[touch_queue]")
{
    td_touch_queue_t *q = fresh_queue();
    const td_touch_sample_t bare = { .x = 300, .y = 200, .pressed = false };
    td_touch_sample_t tap[2];
    td_touch_tap(512, 490, tap);
    TEST_ASSERT_TRUE(td_touch_queue_push(q, tap, 2));

    bool injected = false;
    td_touch_sample_t got = td_touch_next(q, bare, &injected);
    EXPECT_SAMPLE(got, 512, 490, true);
    TEST_ASSERT_TRUE_MESSAGE(injected, "an injected press must win over a released real sample");

    injected = false;
    got = td_touch_next(q, bare, &injected);
    EXPECT_SAMPLE(got, 512, 490, false);
    TEST_ASSERT_TRUE(injected);

    got = td_touch_next(q, bare, &injected);
    EXPECT_SAMPLE(got, 300, 200, false);
    TEST_ASSERT_FALSE(injected);
}

TEST_CASE("with nothing queued the real sample passes through unchanged, released or not", "[touch_queue]")
{
    td_touch_queue_t *q = fresh_queue();
    const td_touch_sample_t pressed = { .x = 1023, .y = 599, .pressed = true };
    const td_touch_sample_t released = { .x = 640, .y = 123, .pressed = false };

    bool injected = true;
    td_touch_sample_t got = td_touch_next(q, pressed, &injected);
    EXPECT_SAMPLE(got, 1023, 599, true);
    TEST_ASSERT_FALSE(injected);

    injected = true;
    got = td_touch_next(q, released, &injected);
    EXPECT_SAMPLE(got, 640, 123, false);
    TEST_ASSERT_FALSE(injected);
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(0, td_touch_queue_count(q), "a real sample passing through must not be queued");
}

TEST_CASE("a leftward swipe is a press, evenly spaced pressed samples and one release", "[touch_queue]")
{
    poison_samples();
    const size_t n = td_touch_swipe(900, 300, 100, 300, 300, s_samples, TD_TOUCH_QUEUE_CAP);
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(32, n, "300 ms is 30 steps, plus the press and the release");

    EXPECT_SAMPLE(s_samples[0], 900, 300, true);
    EXPECT_SAMPLE(s_samples[30], 100, 300, true);
    EXPECT_SAMPLE(s_samples[31], 100, 300, false);

    size_t releases = 0;
    for (size_t i = 0; i < n; i++) {
        releases += !s_samples[i].pressed;
        TEST_ASSERT_EQUAL_UINT16_MESSAGE(300, s_samples[i].y, "a horizontal swipe must keep its y");
    }
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(1, releases, "a swipe ends in exactly one release");

    for (size_t i = 1; i <= 30; i++) {
        const int moved = (int)s_samples[i - 1].x - (int)s_samples[i].x;
        TEST_ASSERT_TRUE_MESSAGE(moved == 26 || moved == 27,
                                 "each step covers 800 px / 30 steps = 26.67 px, give or take rounding");
    }
}

TEST_CASE("an upward swipe moves y in even steps and keeps x", "[touch_queue]")
{
    poison_samples();
    const size_t n = td_touch_swipe(300, 500, 300, 100, 200, s_samples, TD_TOUCH_QUEUE_CAP);
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(22, n, "200 ms is 20 steps, plus the press and the release");

    for (size_t i = 0; i <= 20; i++) {
        EXPECT_SAMPLE(s_samples[i], 300, 500 - 20 * (int)i, true);
    }
    EXPECT_SAMPLE(s_samples[21], 300, 100, false);
}

TEST_CASE("a diagonal swipe moves x and y together in even steps", "[touch_queue]")
{
    poison_samples();
    const size_t n = td_touch_swipe(0, 0, 600, 300, 100, s_samples, TD_TOUCH_QUEUE_CAP);
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(12, n, "100 ms is 10 steps, plus the press and the release");

    for (size_t i = 0; i <= 10; i++) {
        EXPECT_SAMPLE(s_samples[i], 60 * (int)i, 30 * (int)i, true);
    }
    EXPECT_SAMPLE(s_samples[11], 600, 300, false);
}

TEST_CASE("a swipe across the whole coordinate range keeps its endpoints and direction", "[touch_queue]")
{
    poison_samples();
    const size_t n = td_touch_swipe(65535, 0, 0, 65535, TD_TOUCH_SWIPE_MAX_MS, s_samples, TD_TOUCH_QUEUE_CAP);
    TEST_ASSERT_EQUAL_UINT32(202, n);

    EXPECT_SAMPLE(s_samples[0], 65535, 0, true);
    EXPECT_SAMPLE(s_samples[200], 0, 65535, true);
    EXPECT_SAMPLE(s_samples[201], 0, 65535, false);
    for (size_t i = 1; i <= 200; i++) {
        TEST_ASSERT_TRUE_MESSAGE(s_samples[i].x <= s_samples[i - 1].x, "x must never increase on a leftward swipe");
        TEST_ASSERT_TRUE_MESSAGE(s_samples[i].y >= s_samples[i - 1].y, "y must never decrease on a downward swipe");
    }
}

TEST_CASE("a swipe shorter than one step still takes one step", "[touch_queue]")
{
    poison_samples();
    TEST_ASSERT_EQUAL_UINT32(3, td_touch_swipe(10, 20, 30, 40, 5, s_samples, TD_TOUCH_QUEUE_CAP));
    EXPECT_SAMPLE(s_samples[0], 10, 20, true);
    EXPECT_SAMPLE(s_samples[1], 30, 40, true);
    EXPECT_SAMPLE(s_samples[2], 30, 40, false);

    TEST_ASSERT_EQUAL_UINT32_MESSAGE(3, td_touch_swipe(10, 20, 30, 40, 1, s_samples, TD_TOUCH_QUEUE_CAP),
                                     "the shortest allowed swipe is 1 ms");
}

TEST_CASE("a swipe duration rounds down to whole 10 ms steps", "[touch_queue]")
{
    const struct {
        uint32_t ms;
        size_t samples;
    } cases[] = {
        { 9, 3 }, { 10, 3 }, { 19, 3 }, { 20, 4 }, { 25, 4 }, { 29, 4 }, { 30, 5 }, { 1999, 201 },
    };
    for (size_t i = 0; i < sizeof cases / sizeof cases[0]; i++) {
        char message[24];
        snprintf(message, sizeof message, "%u ms", (unsigned)cases[i].ms);
        TEST_ASSERT_EQUAL_UINT32_MESSAGE(
            cases[i].samples, td_touch_swipe(100, 100, 200, 200, cases[i].ms, s_samples, TD_TOUCH_QUEUE_CAP), message);
    }
}

TEST_CASE("a swipe of zero ms or more than the maximum is refused and writes nothing", "[touch_queue]")
{
    poison_samples();
    TEST_ASSERT_EQUAL_UINT32(0, td_touch_swipe(100, 100, 200, 200, 0, s_samples, TD_TOUCH_QUEUE_CAP));
    TEST_ASSERT_EQUAL_UINT32(
        0, td_touch_swipe(100, 100, 200, 200, TD_TOUCH_SWIPE_MAX_MS + 1, s_samples, TD_TOUCH_QUEUE_CAP));
    TEST_ASSERT_EQUAL_UINT32(0, td_touch_swipe(100, 100, 200, 200, UINT32_MAX, s_samples, TD_TOUCH_QUEUE_CAP));
    expect_samples_untouched("a refused swipe must leave the buffer alone");
}

TEST_CASE("a swipe that does not fit the buffer is refused and writes nothing", "[touch_queue]")
{
    poison_samples();
    TEST_ASSERT_EQUAL_UINT32(0, td_touch_swipe(900, 300, 100, 300, 300, s_samples, 31));
    TEST_ASSERT_EQUAL_UINT32(0, td_touch_swipe(900, 300, 100, 300, 300, s_samples, 0));
    expect_samples_untouched("a swipe needing 32 samples must not write into a buffer of 31");

    TEST_ASSERT_EQUAL_UINT32_MESSAGE(32, td_touch_swipe(900, 300, 100, 300, 300, s_samples, 32),
                                     "a buffer of exactly the sample count is enough");
}

TEST_CASE("the longest allowed swipe fits an empty queue", "[touch_queue]")
{
    td_touch_queue_t *q = fresh_queue();
    const size_t n = td_touch_swipe(0, 0, 1023, 599, TD_TOUCH_SWIPE_MAX_MS, s_samples, TD_TOUCH_QUEUE_CAP);
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(202, n, "2000 ms is 200 steps, plus the press and the release");
    EXPECT_SAMPLE(s_samples[200], 1023, 599, true);
    EXPECT_SAMPLE(s_samples[201], 1023, 599, false);

    TEST_ASSERT_TRUE_MESSAGE(td_touch_queue_push(q, s_samples, n), "the longest swipe must fit an empty queue");
    TEST_ASSERT_EQUAL_UINT32(202, td_touch_queue_count(q));
}

TEST_CASE("a full queue rejects new samples without blocking and keeps what it holds", "[touch_queue]")
{
    td_touch_queue_t *q = fresh_queue();
    advance_head(q, 100);
    push_ids(q, 0, TD_TOUCH_QUEUE_CAP);
    TEST_ASSERT_EQUAL_UINT32(TD_TOUCH_QUEUE_CAP, td_touch_queue_count(q));

    const td_touch_sample_t extra = numbered(9999);
    TEST_ASSERT_FALSE_MESSAGE(td_touch_queue_push(q, &extra, 1), "a full queue must refuse one more sample");
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(TD_TOUCH_QUEUE_CAP, td_touch_queue_count(q),
                                     "a refused push must not change the count");

    pop_ids(q, 0, TD_TOUCH_QUEUE_CAP);
    td_touch_sample_t left;
    TEST_ASSERT_FALSE_MESSAGE(td_touch_queue_pop(q, &left), "the refused sample must not have been stored");
}

TEST_CASE("a push that does not fully fit is refused whole", "[touch_queue]")
{
    td_touch_queue_t *q = fresh_queue();
    push_ids(q, 0, TD_TOUCH_QUEUE_CAP - 3);

    fill_ids(TD_TOUCH_QUEUE_CAP - 3, 4);
    TEST_ASSERT_FALSE_MESSAGE(td_touch_queue_push(q, s_samples, 4), "4 samples must not fit in 3 free slots");
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(TD_TOUCH_QUEUE_CAP - 3, td_touch_queue_count(q),
                                     "a refused push must add nothing, not even a part of the batch");

    TEST_ASSERT_TRUE_MESSAGE(td_touch_queue_push(q, s_samples, 3), "3 samples must fit in 3 free slots");
    pop_ids(q, 0, TD_TOUCH_QUEUE_CAP);
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(0, td_touch_queue_count(q), "the refused fourth sample must not be queued");
}

TEST_CASE("a pop from a full queue frees exactly one slot", "[touch_queue]")
{
    td_touch_queue_t *q = fresh_queue();
    advance_head(q, 100);
    push_ids(q, 0, TD_TOUCH_QUEUE_CAP);

    td_touch_sample_t oldest;
    TEST_ASSERT_TRUE(td_touch_queue_pop(q, &oldest));
    expect_numbered(0, oldest);

    const td_touch_sample_t two[] = { numbered(1000), numbered(1001) };
    TEST_ASSERT_FALSE_MESSAGE(td_touch_queue_push(q, two, 2), "one freed slot must not take two samples");
    TEST_ASSERT_TRUE_MESSAGE(td_touch_queue_push(q, two, 1), "one freed slot must take one sample");

    pop_ids(q, 1, TD_TOUCH_QUEUE_CAP - 1);
    td_touch_sample_t newest;
    TEST_ASSERT_TRUE(td_touch_queue_pop(q, &newest));
    expect_numbered(1000, newest);
    TEST_ASSERT_EQUAL_UINT32(0, td_touch_queue_count(q));
}

TEST_CASE("pushing zero samples succeeds, even into a full queue", "[touch_queue]")
{
    td_touch_queue_t *q = fresh_queue();
    TEST_ASSERT_TRUE(td_touch_queue_push(q, s_samples, 0));
    TEST_ASSERT_EQUAL_UINT32(0, td_touch_queue_count(q));

    push_ids(q, 0, TD_TOUCH_QUEUE_CAP);
    TEST_ASSERT_TRUE(td_touch_queue_push(q, s_samples, 0));
    TEST_ASSERT_EQUAL_UINT32(TD_TOUCH_QUEUE_CAP, td_touch_queue_count(q));
}

TEST_CASE("the queue keeps order across the wrap", "[touch_queue]")
{
    td_touch_queue_t *q = fresh_queue();
    uint16_t pushed = 0;
    uint16_t popped = 0;
    push_ids(q, pushed, 100);
    pushed += 100;
    for (int round = 0; round < 10; round++) {
        push_ids(q, pushed, 90);
        pushed += 90;
        pop_ids(q, popped, 90);
        popped += 90;
    }
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(100, td_touch_queue_count(q), "100 samples stay queued while 900 pass through");

    pop_ids(q, popped, 100);
    TEST_ASSERT_EQUAL_UINT32(0, td_touch_queue_count(q));
}

TEST_CASE("popping an empty queue returns false and leaves the output alone", "[touch_queue]")
{
    td_touch_queue_t *q = fresh_queue();
    td_touch_sample_t out = { .x = 0x1234, .y = 0x5678, .pressed = true };
    TEST_ASSERT_FALSE(td_touch_queue_pop(q, &out));
    EXPECT_SAMPLE(out, 0x1234, 0x5678, true);

    push_ids(q, 7, 1);
    pop_ids(q, 7, 1);
    TEST_ASSERT_FALSE_MESSAGE(td_touch_queue_pop(q, &out), "a drained queue is empty again");
    EXPECT_SAMPLE(out, 0x1234, 0x5678, true);
}

TEST_CASE("a tap is a press then a release at the same point", "[touch_queue]")
{
    td_touch_sample_t tap[2];
    memset(tap, POISON, sizeof tap);
    TEST_ASSERT_EQUAL_UINT32(2, td_touch_tap(512, 490, tap));
    EXPECT_SAMPLE(tap[0], 512, 490, true);
    EXPECT_SAMPLE(tap[1], 512, 490, false);
}
