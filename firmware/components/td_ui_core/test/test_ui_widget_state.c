#include <math.h>
#include <stdbool.h>
#include <string.h>
#include <stdio.h>
#include "unity.h"
#include "ui_widget_state.h"

static ui_widget_table_t s_table;

static ui_widget_table_t *table_with(const char *tile, const char *page)
{
    ui_widget_table_clear(&s_table);
    TEST_ASSERT_EQUAL(UI_WIDGET_OK, ui_widget_table_add_tile(&s_table, tile));
    TEST_ASSERT_EQUAL(UI_WIDGET_OK, ui_widget_table_add_page(&s_table, page));
    return &s_table;
}

TEST_CASE("a widget set reads back and marks the tile dirty", "[ui_widget]")
{
    ui_widget_table_t *t = table_with("context", "accounts");
    const ui_widget_state_t *st = ui_widget_get(t, "context");
    TEST_ASSERT_NOT_NULL(st);
    TEST_ASSERT_EQUAL_STRING("", st->value);
    TEST_ASSERT_EQUAL_FLOAT(-1.0f, st->bar);
    TEST_ASSERT_FALSE(t->widgets[0].dirty);

    TEST_ASSERT_EQUAL(UI_WIDGET_OK, ui_widget_set(t, "context", "42%", "context", 0.42f, false, 1234));
    TEST_ASSERT_EQUAL_STRING("42%", st->value);
    TEST_ASSERT_EQUAL_STRING("context", st->caption);
    TEST_ASSERT_FLOAT_WITHIN(0.0001f, 0.42f, st->bar);
    TEST_ASSERT_FALSE(st->stale);
    TEST_ASSERT_EQUAL_UINT32(1234, st->updated_ms);
    TEST_ASSERT_TRUE(t->widgets[0].dirty);
}

TEST_CASE("a widget value longer than 24 characters is cut to 24", "[ui_widget]")
{
    ui_widget_table_t *t = table_with("context", "accounts");
    const char *forty = "0123456789012345678901234567890123456789";
    TEST_ASSERT_EQUAL(UI_WIDGET_OK, ui_widget_set(t, "context", forty, forty, 0.5f, false, 1));
    const ui_widget_state_t *st = ui_widget_get(t, "context");
    TEST_ASSERT_EQUAL_size_t(UI_WIDGET_VALUE_MAX, strlen(st->value));
    TEST_ASSERT_EQUAL_MEMORY(forty, st->value, UI_WIDGET_VALUE_MAX);
    TEST_ASSERT_EQUAL_size_t(UI_WIDGET_CAPTION_MAX, strlen(st->caption));
}

TEST_CASE("a cut never splits a UTF-8 character", "[ui_widget]")
{
    char out[5];
    ui_widget_copy_text(out, sizeof out, "abc\xc3\xa9");
    TEST_ASSERT_EQUAL_STRING("abc", out);
    ui_widget_copy_text(out, sizeof out, NULL);
    TEST_ASSERT_EQUAL_STRING("", out);
}

TEST_CASE("a widget bar is clamped to 0 to 1, and a negative bar means none", "[ui_widget]")
{
    ui_widget_table_t *t = table_with("context", "accounts");
    const ui_widget_state_t *st = ui_widget_get(t, "context");
    ui_widget_set(t, "context", "x", "", 3.0f, false, 1);
    TEST_ASSERT_EQUAL_FLOAT(1.0f, st->bar);
    ui_widget_set(t, "context", "x", "", 0.0f, false, 1);
    TEST_ASSERT_EQUAL_FLOAT(0.0f, st->bar);
    ui_widget_set(t, "context", "x", "", -0.5f, false, 1);
    TEST_ASSERT_EQUAL_FLOAT(-1.0f, st->bar);
    ui_widget_set(t, "context", "x", "", NAN, false, 1);
    TEST_ASSERT_EQUAL_FLOAT(-1.0f, st->bar);
}

TEST_CASE("a widget set for an unknown tile is refused and changes nothing", "[ui_widget]")
{
    ui_widget_table_t *t = table_with("context", "accounts");
    TEST_ASSERT_EQUAL(UI_WIDGET_UNKNOWN, ui_widget_set(t, "ctx", "42%", "", 0.42f, false, 1));
    TEST_ASSERT_EQUAL(UI_WIDGET_UNKNOWN, ui_widget_set(t, NULL, "42%", "", 0.42f, false, 1));
    TEST_ASSERT_NULL(ui_widget_get(t, "ctx"));
    TEST_ASSERT_FALSE(t->widgets[0].dirty);
    TEST_ASSERT_EQUAL_STRING("unknown tile", ui_widget_result_text(UI_WIDGET_UNKNOWN, false));
    TEST_ASSERT_EQUAL(UI_WIDGET_UNKNOWN, ui_dashboard_set(t, "nope", NULL, 0, 1));
}

TEST_CASE("a dashboard keeps 8 rows and refuses a 9th", "[ui_widget]")
{
    ui_widget_table_t *t = table_with("context", "accounts");
    ui_dash_row_t rows[UI_DASH_ROWS_MAX + 1];
    for (int i = 0; i < UI_DASH_ROWS_MAX + 1; i++) {
        snprintf(rows[i].label, sizeof rows[i].label, "row %d", i);
        snprintf(rows[i].value, sizeof rows[i].value, "%d%%", i * 10);
        rows[i].bar = i / 10.0f;
    }
    TEST_ASSERT_EQUAL(UI_WIDGET_OK, ui_dashboard_set(t, "accounts", rows, UI_DASH_ROWS_MAX, 7));
    const ui_dash_state_t *st = ui_dashboard_get(t, "accounts");
    TEST_ASSERT_EQUAL_UINT8(UI_DASH_ROWS_MAX, st->count);
    TEST_ASSERT_EQUAL_STRING("row 7", st->rows[7].label);
    TEST_ASSERT_EQUAL_STRING("70%", st->rows[7].value);
    TEST_ASSERT_TRUE(t->dashboards[0].dirty);

    TEST_ASSERT_EQUAL(UI_WIDGET_TOO_MANY, ui_dashboard_set(t, "accounts", rows, UI_DASH_ROWS_MAX + 1, 8));
    TEST_ASSERT_EQUAL_UINT8(UI_DASH_ROWS_MAX, st->count);
    TEST_ASSERT_EQUAL_UINT32(7, st->updated_ms);
}

TEST_CASE("the stale flag is kept and cleared by the next set", "[ui_widget]")
{
    ui_widget_table_t *t = table_with("context", "accounts");
    ui_widget_set(t, "context", "42%", "", 0.42f, true, 1);
    TEST_ASSERT_TRUE(ui_widget_get(t, "context")->stale);
    ui_widget_set(t, "context", "43%", "", 0.43f, false, 2);
    TEST_ASSERT_FALSE(ui_widget_get(t, "context")->stale);
}

TEST_CASE("a tile id registered twice keeps one entry, and a full table refuses more", "[ui_widget]")
{
    ui_widget_table_clear(&s_table);
    TEST_ASSERT_EQUAL(UI_WIDGET_OK, ui_widget_table_add_tile(&s_table, "a"));
    TEST_ASSERT_EQUAL(UI_WIDGET_OK, ui_widget_table_add_tile(&s_table, "a"));
    TEST_ASSERT_EQUAL_UINT8(1, s_table.widget_count);
    TEST_ASSERT_EQUAL(UI_WIDGET_UNKNOWN, ui_widget_table_add_tile(&s_table, ""));
    for (int i = 1; i < UI_WIDGET_TILES_MAX; i++) {
        char id[8];
        snprintf(id, sizeof id, "t%d", i);
        TEST_ASSERT_EQUAL(UI_WIDGET_OK, ui_widget_table_add_tile(&s_table, id));
    }
    TEST_ASSERT_EQUAL(UI_WIDGET_FULL, ui_widget_table_add_tile(&s_table, "more"));
}
