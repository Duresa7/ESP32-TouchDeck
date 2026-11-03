#include <stdio.h>
#include <string.h>
#include "cJSON.h"
#include "unity.h"
#include "ui_bar.h"

/* schema/live/bar-vectors.json, embedded by the test app with a NUL after it. */
extern const char bar_vectors_start[] asm("_binary_bar_vectors_json_start");

static int num(const cJSON *object, const char *key)
{
    return cJSON_GetObjectItemCaseSensitive(object, key)->valueint;
}

static void assert_rect(const cJSON *want, ui_rect_t got, const char *what)
{
    if (cJSON_IsNull(want)) {
        TEST_ASSERT_EQUAL_INT16_MESSAGE(0, got.w, what);
        TEST_ASSERT_EQUAL_INT16_MESSAGE(0, got.h, what);
        return;
    }
    TEST_ASSERT_EQUAL_INT16_MESSAGE(num(want, "x"), got.x, what);
    TEST_ASSERT_EQUAL_INT16_MESSAGE(num(want, "y"), got.y, what);
    TEST_ASSERT_EQUAL_INT16_MESSAGE(num(want, "w"), got.w, what);
    TEST_ASSERT_EQUAL_INT16_MESSAGE(num(want, "h"), got.h, what);
}

static const char *zone_name(ui_bar_zone_t zone)
{
    return zone == UI_BAR_ZONE_VALUE ? "value" : zone == UI_BAR_ZONE_MUTE ? "mute" : NULL;
}

TEST_CASE("bar: every shared vector places the track, the capture and the mute zone, and hits them", "[ui_bar]")
{
    cJSON *root = cJSON_Parse(bar_vectors_start);
    TEST_ASSERT_NOT_NULL(root);
    const cJSON *constants = cJSON_GetObjectItemCaseSensitive(root, "constants");
    TEST_ASSERT_EQUAL(UI_BAR_PAD, num(constants, "pad"));
    TEST_ASSERT_EQUAL(UI_BAR_GLYPH, num(constants, "glyph_size"));
    TEST_ASSERT_EQUAL(UI_BAR_TRACK_H, num(constants, "track_h"));
    TEST_ASSERT_EQUAL(UI_BAR_CAPTURE_Y, num(constants, "capture_expand_y"));
    int hits = 0;
    const cJSON *c = NULL;
    cJSON_ArrayForEach(c, cJSON_GetObjectItemCaseSensitive(root, "cases"))
    {
        const char *name = cJSON_GetObjectItemCaseSensitive(c, "name")->valuestring;
        const bool muted = cJSON_IsTrue(cJSON_GetObjectItemCaseSensitive(c, "muted_declared"));
        const ui_bar_layout_t layout = ui_bar_layout((int16_t)num(c, "w"), (int16_t)num(c, "h"), muted);
        TEST_ASSERT_TRUE_MESSAGE(layout.zones, name);
        assert_rect(cJSON_GetObjectItemCaseSensitive(c, "track"), layout.track, name);
        assert_rect(cJSON_GetObjectItemCaseSensitive(c, "capture"), layout.capture, name);
        assert_rect(cJSON_GetObjectItemCaseSensitive(c, "mute"), layout.mute, name);
        const cJSON *hit = NULL;
        cJSON_ArrayForEach(hit, cJSON_GetObjectItemCaseSensitive(c, "hits"))
        {
            const cJSON *zone = cJSON_GetObjectItemCaseSensitive(hit, "zone");
            char what[96];
            snprintf(what, sizeof what, "%s at %d,%d", name, num(hit, "x"), num(hit, "y"));
            const ui_bar_zone_t got =
                ui_bar_hit(&layout, (int16_t)num(hit, "x"), (int16_t)num(hit, "y"),
                           cJSON_IsTrue(cJSON_GetObjectItemCaseSensitive(hit, "fresh")),
                           cJSON_IsTrue(cJSON_GetObjectItemCaseSensitive(hit, "input")));
            if (cJSON_IsNull(zone)) {
                TEST_ASSERT_NULL_MESSAGE(zone_name(got), what);
            } else {
                TEST_ASSERT_EQUAL_STRING_MESSAGE(zone->valuestring, zone_name(got), what);
            }
            hits++;
        }
    }
    TEST_ASSERT_TRUE(hits >= 60);
    cJSON_Delete(root);
}

TEST_CASE("bar: a Tile smaller than its glyph and padding has no zones", "[ui_bar]")
{
    const ui_bar_layout_t small = ui_bar_layout(71, 200, true);
    TEST_ASSERT_FALSE(small.zones);
    TEST_ASSERT_EQUAL(UI_BAR_ZONE_NONE, ui_bar_hit(&small, 30, 30, true, true));
    TEST_ASSERT_EQUAL(UI_BAR_ZONE_NONE, ui_bar_hit(&small, 30, 160, true, true));
    TEST_ASSERT_FALSE(ui_bar_layout(200, 71, true).zones);
    TEST_ASSERT_TRUE(ui_bar_layout(72, 72, true).zones);
}

TEST_CASE("bar: the finger's value is its place along the track, clamped, and the fill rounds", "[ui_bar]")
{
    const ui_bar_layout_t l = ui_bar_layout(480, 200, false);
    TEST_ASSERT_EQUAL_DOUBLE(0.0, ui_bar_value_at(&l, 20));
    TEST_ASSERT_EQUAL_DOUBLE(0.0, ui_bar_value_at(&l, -40));
    TEST_ASSERT_EQUAL_DOUBLE(0.5, ui_bar_value_at(&l, 240));
    TEST_ASSERT_EQUAL_DOUBLE(1.0, ui_bar_value_at(&l, 460));
    TEST_ASSERT_EQUAL_DOUBLE(1.0, ui_bar_value_at(&l, 900));
    TEST_ASSERT_EQUAL_INT16(282, ui_bar_fill(&l, 0.64));
    TEST_ASSERT_EQUAL_INT16(0, ui_bar_fill(&l, -1));
    TEST_ASSERT_EQUAL_INT16(440, ui_bar_fill(&l, 3));
}

TEST_CASE("bar: a tap on the track sends nothing, a drag past the slop sends the finger's value", "[ui_bar]")
{
    const ui_bar_layout_t l = ui_bar_layout(480, 200, true);
    ui_bar_t bar;
    ui_bar_init(&bar);
    ui_bar_live(&bar, 0.25);
    ui_bar_press(&bar, 240, 166);
    TEST_ASSERT_FALSE(ui_bar_move(&bar, &l, 248, 166, 10));
    TEST_ASSERT_EQUAL(UI_BAR_LIVE, bar.phase);
    TEST_ASSERT_EQUAL_DOUBLE(0.25, ui_bar_shown(&bar));
    TEST_ASSERT_FALSE(ui_bar_release(&bar, 1000));
    TEST_ASSERT_EQUAL(UI_BAR_LIVE, bar.phase);

    ui_bar_press(&bar, 240, 166);
    TEST_ASSERT_TRUE(ui_bar_move(&bar, &l, 251, 166, 10));
    TEST_ASSERT_EQUAL(UI_BAR_DRAGGING, bar.phase);
    TEST_ASSERT_DOUBLE_WITHIN(1e-9, 0.525, ui_bar_shown(&bar));
    TEST_ASSERT_TRUE(ui_bar_move(&bar, &l, 108, 240, 10));
    TEST_ASSERT_DOUBLE_WITHIN(1e-9, 0.2, ui_bar_shown(&bar));
    TEST_ASSERT_TRUE(ui_bar_release(&bar, 1000));
    TEST_ASSERT_EQUAL(UI_BAR_SETTLING, bar.phase);
    TEST_ASSERT_DOUBLE_WITHIN(1e-9, 0.2, ui_bar_shown(&bar));
}

TEST_CASE("bar: live values during a drag are kept and not drawn, and one within 0.01 confirms the settle",
          "[ui_bar]")
{
    const ui_bar_layout_t l = ui_bar_layout(480, 200, false);
    ui_bar_t bar;
    ui_bar_init(&bar);
    ui_bar_live(&bar, 0.1);
    ui_bar_press(&bar, 20, 166);
    TEST_ASSERT_TRUE(ui_bar_move(&bar, &l, 372, 166, 10));
    ui_bar_live(&bar, 0.3);
    TEST_ASSERT_DOUBLE_WITHIN(1e-9, 0.8, ui_bar_shown(&bar));
    TEST_ASSERT_TRUE(ui_bar_release(&bar, 2000));
    ui_bar_live(&bar, 0.7);
    TEST_ASSERT_EQUAL(UI_BAR_SETTLING, bar.phase);
    TEST_ASSERT_DOUBLE_WITHIN(1e-9, 0.8, ui_bar_shown(&bar));
    ui_bar_live(&bar, 0.7899);
    TEST_ASSERT_EQUAL(UI_BAR_SETTLING, bar.phase);
    ui_bar_live(&bar, 0.81);
    TEST_ASSERT_EQUAL(UI_BAR_LIVE, bar.phase);
    TEST_ASSERT_DOUBLE_WITHIN(1e-9, 0.81, ui_bar_shown(&bar));
}

TEST_CASE("bar: a settle that hears no match gives up after 1.5 s and draws the newest live value", "[ui_bar]")
{
    const ui_bar_layout_t l = ui_bar_layout(480, 200, false);
    ui_bar_t bar;
    ui_bar_init(&bar);
    ui_bar_live(&bar, 0.5);
    ui_bar_press(&bar, 240, 166);
    TEST_ASSERT_TRUE(ui_bar_move(&bar, &l, 460, 166, 10));
    TEST_ASSERT_TRUE(ui_bar_release(&bar, 0xFFFFF000u));
    ui_bar_live(&bar, 0.42);
    TEST_ASSERT_FALSE(ui_bar_tick(&bar, 0xFFFFF000u + UI_BAR_SETTLE_MS - 1));
    TEST_ASSERT_EQUAL(UI_BAR_SETTLING, bar.phase);
    TEST_ASSERT_TRUE(ui_bar_tick(&bar, 0xFFFFF000u + UI_BAR_SETTLE_MS));
    TEST_ASSERT_EQUAL(UI_BAR_LIVE, bar.phase);
    TEST_ASSERT_DOUBLE_WITHIN(1e-9, 0.42, ui_bar_shown(&bar));
    TEST_ASSERT_FALSE(ui_bar_tick(&bar, 0xFFFFF000u + 2 * UI_BAR_SETTLE_MS));
}

TEST_CASE("bar: a lift onto a value the PC already reports is live at once, and a cancel drops the drag", "[ui_bar]")
{
    const ui_bar_layout_t l = ui_bar_layout(480, 200, false);
    ui_bar_t bar;
    ui_bar_init(&bar);
    ui_bar_live(&bar, 0.5);
    ui_bar_press(&bar, 100, 166);
    TEST_ASSERT_TRUE(ui_bar_move(&bar, &l, 240, 166, 10));
    TEST_ASSERT_TRUE(ui_bar_release(&bar, 100));
    TEST_ASSERT_EQUAL(UI_BAR_LIVE, bar.phase);

    ui_bar_press(&bar, 100, 166);
    TEST_ASSERT_TRUE(ui_bar_move(&bar, &l, 400, 166, 10));
    ui_bar_cancel(&bar);
    TEST_ASSERT_EQUAL(UI_BAR_LIVE, bar.phase);
    TEST_ASSERT_FALSE(bar.pressed);
    TEST_ASSERT_DOUBLE_WITHIN(1e-9, 0.5, ui_bar_shown(&bar));
    TEST_ASSERT_FALSE(ui_bar_move(&bar, &l, 410, 166, 10));
    TEST_ASSERT_FALSE(ui_bar_release(&bar, 200));
}
