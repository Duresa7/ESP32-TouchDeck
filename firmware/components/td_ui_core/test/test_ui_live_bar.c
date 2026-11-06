#include <string.h>
#include "unity.h"
#include "ui_live_bar.h"

static const ui_stream_t k_streams[] = { { "value", UI_STREAM_NUMBER }, { "muted", UI_STREAM_NUMBER } };
static const ui_tile_t k_tiles[] = {
    { .id = "vol", .label = "Volume", .row = 0, .col = 0, .row_span = 1, .col_span = 2, .render = UI_RENDER_BAR,
      .input = true, .stream_count = 2, .streams = k_streams },
};
static const ui_page_t k_pages[] = {
    { .id = "media", .title = "Media", .rows = 1, .cols = 2, .tile_count = 1, .tiles = k_tiles },
};

/* The Companion link as the Device wires it: one input mailbox that keeps the latest value and a queue of taps,
 * each sent only while the live table is still at the generation it was tapped in. */
#define TAPS_MAX 8
static struct {
    bool pending;
    double value;
    bool end;
    uint32_t inputs;
    uint32_t cancels;
    uint8_t taps;
    struct {
        char part[16];
        uint32_t generation;
    } tap[TAPS_MAX];
} s_link;

static void link_input(void *ctx, const char *page, const char *tile, double value, bool end)
{
    (void)ctx;
    TEST_ASSERT_EQUAL_STRING("media", page);
    TEST_ASSERT_EQUAL_STRING("vol", tile);
    s_link.pending = true;
    s_link.value = value;
    s_link.end = end;
    s_link.inputs++;
}

static void link_cancel(void *ctx)
{
    (void)ctx;
    s_link.pending = false;
    s_link.cancels++;
}

static void link_part(void *ctx, const char *page, const ui_tile_t *tile, const char *part, uint32_t generation)
{
    (void)ctx;
    TEST_ASSERT_EQUAL_STRING("media", page);
    TEST_ASSERT_EQUAL_PTR(&k_tiles[0], tile);
    TEST_ASSERT_TRUE(s_link.taps < TAPS_MAX);
    strcpy(s_link.tap[s_link.taps].part, part);
    s_link.tap[s_link.taps++].generation = generation;
}

static const ui_live_touch_out_t k_link = { .input = link_input, .cancel = link_cancel, .part = link_part };

static ui_live_slot_t s_slots[2];
static ui_live_table_t s_table;
static ui_live_touch_t s_touch;
static ui_live_bar_t s_bar;
static ui_bar_layout_t s_layout;

static void start(void)
{
    memset(&s_table, 0, sizeof s_table);
    memset(&s_link, 0, sizeof s_link);
    ui_live_bind(&s_table, s_slots, 2, "builtin", k_pages, 1);
    ui_live_session(&s_table, 1000);
    ui_live_touch_init(&s_touch, &k_link);
    ui_live_bar_init(&s_bar, s_table.generation);
    s_layout = ui_bar_layout(480, 200, true);
}

static void value(uint32_t n, double v, uint32_t now_ms)
{
    const ui_live_set_t set = { .layout = "builtin", .page = "media", .tile = "vol", .stream = "value", .n = n,
                                .ttl_ms = 5000, .type = UI_STREAM_NUMBER, .values = &v, .count = 1 };
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, now_ms, NULL));
}

static void feed(void)
{
    ui_live_touch_feed(&s_touch, &s_bar, s_table.generation, ui_live_find(&s_table, &k_tiles[0], "value"));
}

static bool press(int16_t x)
{
    return ui_live_touch_press(&s_touch, &s_bar, &s_layout, "media", &k_tiles[0], s_table.generation, x, 166);
}

static bool move(int16_t x)
{
    return ui_live_touch_move(&s_touch, s_table.generation, x, 166, 10) != NULL;
}

static bool lift(uint32_t now_ms)
{
    return ui_live_touch_release(&s_touch, s_table.generation, now_ms) != NULL;
}

static ui_part_t tap_mute(void)
{
    const uint8_t before = s_link.taps;
    if (!ui_live_touch_tap_bar(&s_touch, &s_bar, &s_layout, "media", &k_tiles[0], s_table.generation, 30, 30)) {
        TEST_ASSERT_EQUAL_UINT8(before, s_link.taps);
        return UI_PART_NONE;
    }
    TEST_ASSERT_EQUAL_UINT8(before + 1, s_link.taps);
    TEST_ASSERT_EQUAL_STRING("mute", s_link.tap[before].part);
    TEST_ASSERT_EQUAL_UINT32(s_table.generation, s_link.tap[before].generation);
    return UI_PART_MUTE;
}

/* The taps the link sends now: those still at the live table's generation. */
static uint8_t send_taps(void)
{
    uint8_t sent = 0;
    for (uint8_t i = 0; i < s_link.taps; i++) {
        sent = (uint8_t)(sent + (s_link.tap[i].generation == s_table.generation));
    }
    s_link.taps = 0;
    return sent;
}

/* A new hello, as the Device handles it: the table starts a session and the touch resets. */
static void new_session(uint32_t now_ms)
{
    ui_live_session(&s_table, now_ms);
    ui_live_touch_reset(&s_touch);
}

TEST_CASE("live bar: a session reset mid-drag sends no move and no end afterwards", "[ui_live_bar]")
{
    start();
    value(1, 0.4, 1000);
    feed();
    TEST_ASSERT_TRUE(press(100));
    TEST_ASSERT_TRUE(move(372));
    ui_live_session(&s_table, 2000);
    TEST_ASSERT_FALSE(move(400));
    TEST_ASSERT_FALSE(lift(2100));
    TEST_ASSERT_EQUAL(UI_BAR_LIVE, s_bar.bar.phase);
    TEST_ASSERT_FALSE(s_bar.bar.pressed);

    value(1, 0.4, 2200);
    feed();
    TEST_ASSERT_TRUE(press(100));
    TEST_ASSERT_TRUE(move(372));
    ui_live_session(&s_table, 3000);
    TEST_ASSERT_FALSE(lift(3100));
    TEST_ASSERT_EQUAL(UI_BAR_LIVE, s_bar.bar.phase);
}

TEST_CASE("live bar: a reset followed at once by a value with the same n and digest is fed", "[ui_live_bar]")
{
    start();
    value(1, 0.4, 1000);
    feed();
    TEST_ASSERT_TRUE(press(100));
    TEST_ASSERT_TRUE(move(372));
    TEST_ASSERT_TRUE(lift(1100));
    TEST_ASSERT_EQUAL(UI_BAR_SETTLING, s_bar.bar.phase);
    const uint32_t digest = s_bar.fed_digest;
    ui_live_session(&s_table, 1200);
    value(1, 0.4, 1200);
    TEST_ASSERT_EQUAL_UINT32(digest, ui_live_find(&s_table, &k_tiles[0], "value")->digest);
    feed();
    TEST_ASSERT_EQUAL(UI_BAR_LIVE, s_bar.bar.phase);
    TEST_ASSERT_DOUBLE_WITHIN(1e-9, 0.4, ui_bar_shown(&s_bar.bar));
    TEST_ASSERT_EQUAL_UINT32(1, s_bar.fed_n);
}

TEST_CASE("live bar: a press after a reset the screen has not drawn yet holds nothing", "[ui_live_bar]")
{
    start();
    value(1, 0.4, 1000);
    feed();
    ui_live_session(&s_table, 2000);
    TEST_ASSERT_FALSE(press(100));
    TEST_ASSERT_FALSE(move(372));
    TEST_ASSERT_FALSE(lift(2100));
    TEST_ASSERT_DOUBLE_WITHIN(1e-9, 0.0, ui_bar_shown(&s_bar.bar));
}

TEST_CASE("live bar: a new Layout ends a drag too", "[ui_live_bar]")
{
    start();
    value(1, 0.4, 1000);
    feed();
    TEST_ASSERT_TRUE(press(100));
    TEST_ASSERT_TRUE(move(372));
    ui_live_bind(&s_table, s_slots, 2, "other", k_pages, 1);
    TEST_ASSERT_FALSE(move(400));
    TEST_ASSERT_FALSE(lift(1100));
}

TEST_CASE("live bar: each value is fed once, and an unset value lets the next one in whatever its n", "[ui_live_bar]")
{
    start();
    value(1, 0.4, 1000);
    feed();
    TEST_ASSERT_TRUE(press(100));
    TEST_ASSERT_TRUE(move(372));
    TEST_ASSERT_TRUE(lift(1100));
    feed();
    TEST_ASSERT_EQUAL(UI_BAR_SETTLING, s_bar.bar.phase);
    value(2, 0.8, 1600);
    feed();
    TEST_ASSERT_EQUAL(UI_BAR_LIVE, s_bar.bar.phase);
    TEST_ASSERT_DOUBLE_WITHIN(1e-9, 0.8, ui_bar_shown(&s_bar.bar));
    ui_live_touch_feed(&s_touch, &s_bar, s_table.generation, NULL);
    TEST_ASSERT_EQUAL_UINT32(0, s_bar.fed_n);
    feed();
    TEST_ASSERT_EQUAL_UINT32(2, s_bar.fed_n);
}

TEST_CASE("live bar: a mute tap after a reset names no part until the new session feeds a fresh value", "[ui_live_bar]")
{
    start();
    value(1, 0.4, 1000);
    feed();
    TEST_ASSERT_EQUAL(UI_PART_MUTE, tap_mute());
    ui_live_session(&s_table, 2000);
    TEST_ASSERT_EQUAL(UI_PART_NONE, tap_mute());
    TEST_ASSERT_FALSE(press(100));
    TEST_ASSERT_EQUAL(UI_PART_NONE, tap_mute());
    feed();
    TEST_ASSERT_EQUAL(UI_PART_NONE, tap_mute());
    value(1, 0.4, 2100);
    feed();
    TEST_ASSERT_EQUAL(UI_PART_MUTE, tap_mute());
    ui_live_bind(&s_table, s_slots, 2, "other", k_pages, 1);
    TEST_ASSERT_EQUAL(UI_PART_NONE, tap_mute());
}

/* Expires the value and feeds it. True when that ended the press, drag or settle the bar held. */
static bool stale(uint32_t now_ms)
{
    TEST_ASSERT_TRUE(ui_live_expire(&s_table, now_ms));
    const bool holding = s_bar.bar.pressed || s_bar.bar.phase != UI_BAR_LIVE;
    feed();
    return holding && !s_bar.bar.pressed && s_bar.bar.phase == UI_BAR_LIVE;
}

TEST_CASE("live bar: a value going stale mid-drag ends the drag, sends nothing more and shows the stale value",
          "[ui_live_bar]")
{
    start();
    value(1, 0.4, 1000);
    feed();
    TEST_ASSERT_TRUE(press(100));
    TEST_ASSERT_TRUE(move(372));
    TEST_ASSERT_TRUE(stale(6000));
    TEST_ASSERT_FALSE(s_bar.fresh);
    TEST_ASSERT_FALSE(move(400));
    TEST_ASSERT_FALSE(lift(6100));
    TEST_ASSERT_EQUAL(UI_BAR_LIVE, s_bar.bar.phase);
    TEST_ASSERT_FALSE(s_bar.bar.pressed);
    TEST_ASSERT_DOUBLE_WITHIN(1e-9, 0.4, ui_bar_shown(&s_bar.bar));
    TEST_ASSERT_EQUAL(UI_PART_NONE, tap_mute());

    value(2, 0.6, 7000);
    feed();
    TEST_ASSERT_TRUE(press(100));
    TEST_ASSERT_TRUE(move(372));
    TEST_ASSERT_TRUE(lift(7100));
}

TEST_CASE("live bar: a value going stale before the press passes the slop ends the press", "[ui_live_bar]")
{
    start();
    value(1, 0.4, 1000);
    feed();
    TEST_ASSERT_TRUE(press(100));
    TEST_ASSERT_FALSE(move(105));
    TEST_ASSERT_TRUE(stale(6000));
    TEST_ASSERT_FALSE(move(372));
    TEST_ASSERT_FALSE(lift(6100));
    TEST_ASSERT_DOUBLE_WITHIN(1e-9, 0.4, ui_bar_shown(&s_bar.bar));
}

TEST_CASE("live bar: a value going stale while the bar settles shows the stale value", "[ui_live_bar]")
{
    start();
    value(1, 0.4, 1000);
    feed();
    TEST_ASSERT_TRUE(press(100));
    TEST_ASSERT_TRUE(move(372));
    TEST_ASSERT_TRUE(lift(1100));
    TEST_ASSERT_EQUAL(UI_BAR_SETTLING, s_bar.bar.phase);
    TEST_ASSERT_TRUE(stale(6000));
    TEST_ASSERT_EQUAL(UI_BAR_LIVE, s_bar.bar.phase);
    TEST_ASSERT_DOUBLE_WITHIN(1e-9, 0.4, ui_bar_shown(&s_bar.bar));
}

TEST_CASE("live bar: a fresh value mid-drag keeps the drag going", "[ui_live_bar]")
{
    start();
    value(1, 0.4, 1000);
    feed();
    TEST_ASSERT_TRUE(press(100));
    TEST_ASSERT_TRUE(move(372));
    value(2, 0.5, 1200);
    feed();
    TEST_ASSERT_TRUE(ui_live_touch_holds(&s_touch, &s_bar));
    TEST_ASSERT_EQUAL_UINT32(0, s_link.cancels);
    TEST_ASSERT_TRUE(s_bar.fresh);
    TEST_ASSERT_TRUE(move(400));
    TEST_ASSERT_EQUAL(UI_BAR_DRAGGING, s_bar.bar.phase);
    TEST_ASSERT_TRUE(lift(1300));
}

TEST_CASE("live bar: a value going stale on a bar nobody holds ends nothing", "[ui_live_bar]")
{
    start();
    value(1, 0.4, 1000);
    feed();
    TEST_ASSERT_FALSE(stale(6000));
    TEST_ASSERT_FALSE(s_bar.fresh);
    TEST_ASSERT_DOUBLE_WITHIN(1e-9, 0.4, ui_bar_shown(&s_bar.bar));
}

TEST_CASE("live touch: a drag cut off by a stale value sends no more moves or end and drops its queued input",
          "[ui_live_bar]")
{
    start();
    value(1, 0.4, 1000);
    feed();
    TEST_ASSERT_TRUE(press(100));
    TEST_ASSERT_TRUE(move(372));
    TEST_ASSERT_TRUE(s_link.pending);
    TEST_ASSERT_FALSE(s_link.end);
    TEST_ASSERT_TRUE(stale(6000));
    TEST_ASSERT_FALSE(ui_live_touch_holds(&s_touch, &s_bar));
    TEST_ASSERT_EQUAL_UINT32(1, s_link.cancels);
    TEST_ASSERT_FALSE(s_link.pending);
    TEST_ASSERT_FALSE(move(400));
    TEST_ASSERT_FALSE(lift(6100));
    TEST_ASSERT_EQUAL_UINT32(1, s_link.inputs);
    TEST_ASSERT_FALSE(s_link.pending);
}

TEST_CASE("live touch: a value going stale while the bar settles keeps the end the lift queued", "[ui_live_bar]")
{
    start();
    value(1, 0.4, 1000);
    feed();
    TEST_ASSERT_TRUE(press(100));
    TEST_ASSERT_TRUE(move(372));
    TEST_ASSERT_TRUE(lift(1100));
    TEST_ASSERT_TRUE(s_link.end);
    TEST_ASSERT_TRUE(stale(6000));
    TEST_ASSERT_EQUAL_UINT32(0, s_link.cancels);
    TEST_ASSERT_TRUE(s_link.pending);
    TEST_ASSERT_TRUE(s_link.end);
}

TEST_CASE("live touch: a press on another bar ends the held drag and drops its queued input", "[ui_live_bar]")
{
    start();
    value(1, 0.4, 1000);
    feed();
    TEST_ASSERT_TRUE(press(100));
    TEST_ASSERT_TRUE(move(372));
    TEST_ASSERT_FALSE(ui_live_touch_press(&s_touch, NULL, NULL, "media", &k_tiles[0], s_table.generation, 0, 0));
    TEST_ASSERT_EQUAL_UINT32(1, s_link.cancels);
    TEST_ASSERT_FALSE(s_link.pending);
    TEST_ASSERT_FALSE(s_bar.bar.pressed);
    TEST_ASSERT_EQUAL(UI_BAR_LIVE, s_bar.bar.phase);
    TEST_ASSERT_FALSE(move(400));
    TEST_ASSERT_FALSE(lift(1100));
}

TEST_CASE("live touch: a session reset drops a pending tap and input, and the next touch works", "[ui_live_bar]")
{
    start();
    value(1, 0.4, 1000);
    feed();
    TEST_ASSERT_EQUAL(UI_PART_MUTE, tap_mute());
    TEST_ASSERT_TRUE(press(100));
    TEST_ASSERT_TRUE(move(372));
    TEST_ASSERT_TRUE(s_link.pending);

    new_session(2000);
    TEST_ASSERT_FALSE(s_link.pending);
    TEST_ASSERT_EQUAL_UINT32(1, s_link.cancels);
    TEST_ASSERT_FALSE(ui_live_touch_holds(&s_touch, &s_bar));
    TEST_ASSERT_EQUAL_UINT8(0, send_taps());
    TEST_ASSERT_FALSE(move(400));
    TEST_ASSERT_FALSE(lift(2050));
    TEST_ASSERT_EQUAL_UINT32(1, s_link.inputs);
    TEST_ASSERT_FALSE(s_link.pending);

    value(1, 0.6, 2100);
    feed();
    TEST_ASSERT_EQUAL(UI_PART_MUTE, tap_mute());
    TEST_ASSERT_EQUAL_UINT8(1, send_taps());
    TEST_ASSERT_TRUE(press(100));
    TEST_ASSERT_TRUE(move(372));
    TEST_ASSERT_TRUE(s_link.pending);
    TEST_ASSERT_FALSE(s_link.end);
    TEST_ASSERT_TRUE(lift(2200));
    TEST_ASSERT_TRUE(s_link.end);
    TEST_ASSERT_EQUAL_UINT32(3, s_link.inputs);
    TEST_ASSERT_EQUAL_UINT32(1, s_link.cancels);
}

TEST_CASE("live touch: a tap resolved against an older generation is never sent", "[ui_live_bar]")
{
    start();
    value(1, 0.4, 1000);
    feed();
    TEST_ASSERT_EQUAL(UI_PART_MUTE, tap_mute());
    ui_live_bind(&s_table, s_slots, 2, "other", k_pages, 1);
    TEST_ASSERT_EQUAL_UINT8(0, send_taps());

    TEST_ASSERT_EQUAL(UI_PART_NONE, tap_mute());
    new_session(2000);
    TEST_ASSERT_EQUAL(UI_PART_NONE, tap_mute());
    TEST_ASSERT_EQUAL_UINT8(0, s_link.taps);
}
