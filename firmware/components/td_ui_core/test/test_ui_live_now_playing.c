#include <string.h>
#include "unity.h"
#include "ui_live_now_playing.h"

static const ui_stream_t k_streams[] = { { "state", UI_STREAM_NUMBER } };
static const ui_tile_t k_tiles[] = {
    { .id = "np", .label = "Now playing", .row = 0, .col = 0, .row_span = 1, .col_span = 2,
      .render = UI_RENDER_NOW_PLAYING, .stream_count = 1, .streams = k_streams },
};
static const ui_page_t k_pages[] = {
    { .id = "media", .title = "Media", .rows = 1, .cols = 2, .tile_count = 1, .tiles = k_tiles },
};

static ui_live_slot_t s_slots[1];
static ui_live_table_t s_table;
static ui_live_np_t s_np;
static ui_np_layout_t s_layout;

static void start(void)
{
    memset(&s_table, 0, sizeof s_table);
    ui_live_bind(&s_table, s_slots, 1, "builtin", k_pages, 1);
    ui_live_session(&s_table, 1000);
    s_np = (ui_live_np_t){ 0 };
    s_layout = ui_np_layout(480, 200);
}

static void state(uint32_t n, double v, uint32_t now_ms)
{
    const ui_live_set_t set = { .layout = "builtin", .page = "media", .tile = "np", .stream = "state", .n = n,
                                .ttl_ms = 5000, .type = UI_STREAM_NUMBER, .values = &v, .count = 1 };
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, now_ms, NULL));
}

static void feed(void)
{
    ui_live_np_feed(&s_np, s_table.generation, ui_live_find(&s_table, &k_tiles[0], "state"));
}

static ui_part_t tap(uint8_t zone)
{
    const ui_rect_t r = s_layout.zones[zone];
    return ui_live_np_part(&s_np, s_table.generation, &s_layout, (int16_t)(r.x + r.w / 2), (int16_t)(r.y + r.h / 2));
}

TEST_CASE("live now playing: a tap after a reset names no part until the new session feeds a fresh state",
          "[ui_live_now_playing]")
{
    start();
    TEST_ASSERT_TRUE(s_layout.controls);
    state(1, UI_NP_PLAYING, 1000);
    feed();
    TEST_ASSERT_EQUAL(UI_PART_PREVIOUS, tap(0));
    TEST_ASSERT_EQUAL(UI_PART_PLAY_PAUSE, tap(1));
    TEST_ASSERT_EQUAL(UI_PART_NEXT, tap(2));
    ui_live_session(&s_table, 2000);
    for (uint8_t i = 0; i < UI_NP_PARTS_MAX; i++) {
        TEST_ASSERT_EQUAL(UI_PART_NONE, tap(i));
    }
    feed();
    TEST_ASSERT_EQUAL(UI_PART_NONE, tap(1));
    state(1, UI_NP_PAUSED, 2100);
    feed();
    TEST_ASSERT_EQUAL(UI_PART_PREVIOUS, tap(0));
    TEST_ASSERT_EQUAL(UI_PART_PLAY_PAUSE, tap(1));
    TEST_ASSERT_EQUAL(UI_PART_NEXT, tap(2));
    ui_live_bind(&s_table, s_slots, 1, "other", k_pages, 1);
    TEST_ASSERT_EQUAL(UI_PART_NONE, tap(1));
}
