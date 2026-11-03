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

static ui_live_slot_t s_slots[2];
static ui_live_table_t s_table;
static ui_live_bar_t s_bar;
static ui_bar_layout_t s_layout;

static void start(void)
{
    memset(&s_table, 0, sizeof s_table);
    ui_live_bind(&s_table, s_slots, 2, "builtin", k_pages, 1);
    ui_live_session(&s_table, 1000);
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
    ui_live_bar_feed(&s_bar, s_table.generation, ui_live_find(&s_table, &k_tiles[0], "value"));
}

static bool press(int16_t x)
{
    return ui_live_bar_press(&s_bar, s_table.generation, x, 166);
}

static bool move(int16_t x)
{
    return ui_live_bar_move(&s_bar, s_table.generation, &s_layout, x, 166, 10);
}

static bool lift(uint32_t now_ms)
{
    return ui_live_bar_release(&s_bar, s_table.generation, now_ms);
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
    ui_live_bar_feed(&s_bar, s_table.generation, NULL);
    TEST_ASSERT_EQUAL_UINT32(0, s_bar.fed_n);
    feed();
    TEST_ASSERT_EQUAL_UINT32(2, s_bar.fed_n);
}
