#include <stdio.h>
#include <string.h>
#include "unity.h"
#include "ui_live.h"

static const ui_stream_t k_number[] = { { "value", UI_STREAM_NUMBER } };
static const ui_stream_t k_series[] = { { "value", UI_STREAM_SERIES } };
static const ui_tile_t k_tiles[] = {
    { .id = "cpu", .label = "CPU", .row = 0, .col = 0, .row_span = 1, .col_span = 1, .render = UI_RENDER_GRAPH,
      .stream_count = 1, .streams = k_number },
    { .id = "net", .label = "Net", .row = 0, .col = 1, .row_span = 1, .col_span = 1, .render = UI_RENDER_GRAPH,
      .stream_count = 1, .streams = k_series },
    { .id = "key", .label = "Key", .row = 1, .col = 0, .row_span = 1, .col_span = 1 },
};
static const ui_page_t k_pages[] = {
    { .id = "main", .title = "Main", .rows = 2, .cols = 2, .tile_count = 3, .tiles = k_tiles },
};

static ui_live_slot_t s_slots[4];
static ui_live_table_t s_table;

static void bind(uint16_t cap)
{
    memset(&s_table, 0, sizeof s_table);
    ui_live_bind(&s_table, s_slots, cap, "builtin", k_pages, 1);
    ui_live_session(&s_table, 1000);
}

static ui_live_result_t number(const char *tile, uint32_t n, double value, uint32_t now_ms, uint32_t *retry)
{
    const ui_live_set_t set = { .layout = "builtin", .page = "main", .tile = tile, .stream = "value", .n = n,
                                .ttl_ms = 5000, .type = UI_STREAM_NUMBER, .values = &value, .count = 1 };
    return ui_live_set(&s_table, &set, now_ms, retry);
}

TEST_CASE("live: a Layout binds one slot per declared stream", "[ui_live]")
{
    bind(4);
    TEST_ASSERT_EQUAL_UINT(2, ui_live_streams(k_pages, 1));
    TEST_ASSERT_EQUAL_UINT16(2, s_table.count);
    TEST_ASSERT_EQUAL_PTR(&k_tiles[0], s_table.slots[0].tile);
    TEST_ASSERT_FALSE(s_table.slots[0].set);
    bind(1);
    TEST_ASSERT_EQUAL_UINT16(1, s_table.count);
    TEST_ASSERT_EQUAL_UINT16(1, s_table.dropped);
    double values[] = { 1 };
    const ui_live_set_t set = { .layout = "builtin", .page = "main", .tile = "net", .stream = "value", .n = 1,
                                .ttl_ms = 5000, .type = UI_STREAM_SERIES, .values = values, .count = 1 };
    TEST_ASSERT_EQUAL(UI_LIVE_MEMORY, ui_live_set(&s_table, &set, 1000, NULL));
}

TEST_CASE("live: a number appends and keeps the newest UI_LIVE_SERIES_MAX samples", "[ui_live]")
{
    bind(4);
    double out[UI_LIVE_SERIES_MAX];
    for (uint32_t i = 1; i <= UI_LIVE_SERIES_MAX + 3; i++) {
        TEST_ASSERT_EQUAL(UI_LIVE_OK, number("cpu", i, (double)i, 1000 + 1000 * i, NULL));
    }
    const ui_live_slot_t *slot = ui_live_find(&s_table, &k_tiles[0], "value");
    TEST_ASSERT_EQUAL_UINT8(UI_LIVE_SERIES_MAX, ui_live_samples(slot, out));
    TEST_ASSERT_EQUAL_DOUBLE(4.0, out[0]);
    TEST_ASSERT_EQUAL_DOUBLE((double)(UI_LIVE_SERIES_MAX + 3), out[UI_LIVE_SERIES_MAX - 1]);
}

TEST_CASE("live: a series replaces the history and its length is bounded", "[ui_live]")
{
    bind(4);
    double values[UI_LIVE_SERIES_MAX + 1];
    for (int i = 0; i <= UI_LIVE_SERIES_MAX; i++) {
        values[i] = i;
    }
    ui_live_set_t set = { .layout = "builtin", .page = "main", .tile = "net", .stream = "value", .n = 1,
                          .ttl_ms = 5000, .type = UI_STREAM_SERIES, .values = values, .count = 3 };
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, 1000, NULL));
    set.n = 2;
    set.values = values + 10;
    set.count = 2;
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, 1600, NULL));
    double out[UI_LIVE_SERIES_MAX];
    TEST_ASSERT_EQUAL_UINT8(2, ui_live_samples(ui_live_find(&s_table, &k_tiles[1], "value"), out));
    TEST_ASSERT_EQUAL_DOUBLE(10.0, out[0]);
    set.n = 3;
    set.values = values;
    set.count = UI_LIVE_SERIES_MAX + 1;
    TEST_ASSERT_EQUAL(UI_LIVE_SIZE, ui_live_set(&s_table, &set, 3000, NULL));
    set.count = 0;
    TEST_ASSERT_EQUAL(UI_LIVE_ARGS, ui_live_set(&s_table, &set, 3000, NULL));
}

TEST_CASE("live: targets, types and values are checked before anything changes", "[ui_live]")
{
    bind(4);
    double v = 1;
    ui_live_set_t set = { .layout = "sha256", .page = "main", .tile = "cpu", .stream = "value", .n = 1,
                          .ttl_ms = 5000, .type = UI_STREAM_NUMBER, .values = &v, .count = 1 };
    TEST_ASSERT_EQUAL(UI_LIVE_LAYOUT, ui_live_set(&s_table, &set, 1000, NULL));
    set.layout = "builtin";
    set.page = "other";
    TEST_ASSERT_EQUAL(UI_LIVE_PAGE, ui_live_set(&s_table, &set, 1000, NULL));
    set.page = "main";
    set.tile = "nope";
    TEST_ASSERT_EQUAL(UI_LIVE_TILE, ui_live_set(&s_table, &set, 1000, NULL));
    set.tile = "key";
    TEST_ASSERT_EQUAL(UI_LIVE_STREAM, ui_live_set(&s_table, &set, 1000, NULL));
    set.tile = "cpu";
    set.type = UI_STREAM_SERIES;
    TEST_ASSERT_EQUAL(UI_LIVE_TYPE, ui_live_set(&s_table, &set, 1000, NULL));
    set.type = UI_STREAM_NUMBER;
    v = 1e9 + 1;
    TEST_ASSERT_EQUAL(UI_LIVE_ARGS, ui_live_set(&s_table, &set, 1000, NULL));
    v = 1;
    set.ttl_ms = 999;
    TEST_ASSERT_EQUAL(UI_LIVE_ARGS, ui_live_set(&s_table, &set, 1000, NULL));
    TEST_ASSERT_FALSE(ui_live_find(&s_table, &k_tiles[0], "value")->set);
    TEST_ASSERT_EQUAL_STRING("sequence", ui_live_reason(UI_LIVE_SEQUENCE));
    TEST_ASSERT_NULL(ui_live_reason(UI_LIVE_OK));
}

TEST_CASE("live: n only grows, and an identical retry changes nothing", "[ui_live]")
{
    bind(4);
    TEST_ASSERT_EQUAL(UI_LIVE_OK, number("cpu", 5, 10, 1000, NULL));
    const ui_live_slot_t *slot = ui_live_find(&s_table, &k_tiles[0], "value");
    const uint32_t expires = slot->expires_ms;
    TEST_ASSERT_EQUAL(UI_LIVE_OK, number("cpu", 5, 10, 1300, NULL));
    TEST_ASSERT_EQUAL_UINT32(expires, slot->expires_ms);
    TEST_ASSERT_EQUAL_UINT8(1, slot->count);
    TEST_ASSERT_EQUAL(UI_LIVE_SEQUENCE, number("cpu", 5, 11, 1300, NULL));
    TEST_ASSERT_EQUAL(UI_LIVE_SEQUENCE, number("cpu", 4, 10, 2600, NULL));
    TEST_ASSERT_EQUAL(UI_LIVE_OK, number("cpu", 6, 12, 2600, NULL));
    ui_live_session(&s_table, 5000);
    TEST_ASSERT_FALSE(slot->set);
    TEST_ASSERT_EQUAL(UI_LIVE_OK, number("cpu", 1, 1, 5000, NULL));
}

TEST_CASE("live: per-stream and per-Device windows refuse with the time to the next window", "[ui_live]")
{
    bind(4);
    uint32_t retry = 0;
    for (uint32_t n = 1; n <= UI_LIVE_HZ; n++) {
        TEST_ASSERT_EQUAL(UI_LIVE_OK, number("cpu", n, 1, 1100, &retry));
    }
    TEST_ASSERT_EQUAL(UI_LIVE_RATE, number("cpu", 10, 1, 1250, &retry));
    TEST_ASSERT_EQUAL_UINT32(750, retry);
    const ui_live_slot_t *slot = ui_live_find(&s_table, &k_tiles[0], "value");
    TEST_ASSERT_EQUAL_UINT32(UI_LIVE_HZ, slot->n);
    TEST_ASSERT_EQUAL(UI_LIVE_OK, number("cpu", 10, 1, 2000, &retry));
}

TEST_CASE("live: one update a second is never throttled", "[ui_live]")
{
    bind(4);
    uint32_t retry = 0;
    for (uint32_t i = 0; i < 10; i++) {
        TEST_ASSERT_EQUAL(UI_LIVE_OK, number("cpu", i + 1, (double)i, 1000 + 1000 * i, &retry));
    }
}

TEST_CASE("live: the Device window caps all streams together", "[ui_live]")
{
    static ui_stream_t streams[UI_LIVE_TOTAL_HZ + 1];
    static ui_tile_t tiles[UI_LIVE_TOTAL_HZ + 1];
    static char ids[UI_LIVE_TOTAL_HZ + 1][8];
    for (int i = 0; i <= UI_LIVE_TOTAL_HZ; i++) {
        streams[i] = (ui_stream_t){ "value", UI_STREAM_NUMBER };
        snprintf(ids[i], sizeof ids[i], "t%d", i);
        tiles[i] = (ui_tile_t){ .id = ids[i], .label = ids[i], .row = (uint8_t)(i / 8), .col = (uint8_t)(i % 8),
                                .row_span = 1, .col_span = 1, .render = UI_RENDER_GRAPH, .stream_count = 1,
                                .streams = &streams[i] };
    }
    const ui_page_t page = { .id = "main", .title = "Main", .rows = 8, .cols = 8, .tile_count = UI_LIVE_TOTAL_HZ + 1,
                             .tiles = tiles };
    static ui_live_slot_t slots[UI_LIVE_TOTAL_HZ + 1];
    ui_live_table_t table = { 0 };
    ui_live_bind(&table, slots, UI_LIVE_TOTAL_HZ + 1, "builtin", &page, 1);
    ui_live_session(&table, 0);
    double v = 1;
    uint32_t retry = 0;
    for (int i = 0; i <= UI_LIVE_TOTAL_HZ; i++) {
        const ui_live_set_t set = { .layout = "builtin", .page = "main", .tile = ids[i], .stream = "value", .n = 1,
                                    .ttl_ms = 5000, .type = UI_STREAM_NUMBER, .values = &v, .count = 1 };
        TEST_ASSERT_EQUAL(i < UI_LIVE_TOTAL_HZ ? UI_LIVE_OK : UI_LIVE_RATE, ui_live_set(&table, &set, 400, &retry));
    }
    TEST_ASSERT_EQUAL_UINT32(600, retry);
    TEST_ASSERT_FALSE(slots[UI_LIVE_TOTAL_HZ].set);
}

TEST_CASE("live: a value goes stale at its ttl or offline, keeps its samples, and a fresh one clears it", "[ui_live]")
{
    bind(4);
    TEST_ASSERT_EQUAL(UI_LIVE_OK, number("cpu", 1, 42, 1000, NULL));
    ui_live_slot_t *slot = ui_live_find(&s_table, &k_tiles[0], "value");
    slot->dirty = false;
    TEST_ASSERT_FALSE(ui_live_expire(&s_table, 5999));
    TEST_ASSERT_FALSE(slot->stale);
    TEST_ASSERT_TRUE(ui_live_expire(&s_table, 6000));
    TEST_ASSERT_TRUE(slot->stale);
    TEST_ASSERT_TRUE(slot->dirty);
    TEST_ASSERT_EQUAL_UINT8(1, slot->count);
    TEST_ASSERT_EQUAL_DOUBLE(42.0, slot->samples[slot->head]);
    TEST_ASSERT_FALSE(ui_live_expire(&s_table, 9000));
    TEST_ASSERT_EQUAL(UI_LIVE_OK, number("cpu", 2, 43, 9000, NULL));
    TEST_ASSERT_FALSE(slot->stale);
    TEST_ASSERT_EQUAL_UINT8(2, slot->count);
    TEST_ASSERT_TRUE(ui_live_offline(&s_table));
    TEST_ASSERT_TRUE(slot->stale);
    TEST_ASSERT_FALSE(ui_live_find(&s_table, &k_tiles[1], "value")->stale);
}
