#include <stdbool.h>
#include <stddef.h>
#include "unity.h"
#include "ui_redraw.h"

TEST_CASE("the same text in another buffer skips the set", "[ui_redraw]")
{
    char shown[] = "42%";
    char empty[] = "";
    char fallback[] = "--";
    TEST_ASSERT_TRUE(ui_text_unchanged(shown, "42%"));
    TEST_ASSERT_TRUE(ui_text_unchanged(empty, ""));
    TEST_ASSERT_TRUE(ui_text_unchanged(fallback, "--"));
}

TEST_CASE("any difference in the text takes the set", "[ui_redraw]")
{
    char shown[] = "42%";
    TEST_ASSERT_FALSE(ui_text_unchanged(shown, "43%"));
    TEST_ASSERT_FALSE(ui_text_unchanged(shown, "42"));
    TEST_ASSERT_FALSE(ui_text_unchanged(shown, "42%%"));
    TEST_ASSERT_FALSE(ui_text_unchanged(shown, ""));
    char caption[] = "cpu";
    TEST_ASSERT_FALSE(ui_text_unchanged(caption, "CPU"));
    char empty[] = "";
    TEST_ASSERT_FALSE(ui_text_unchanged(empty, "--"));
}

TEST_CASE("a label showing the caller's own static string still takes the set", "[ui_redraw]")
{
    /* A widget caption starts as the tile label itself, and its first redraw must still make LVGL copy it. */
    static const char tile_label[] = "Context";
    TEST_ASSERT_FALSE(ui_text_unchanged(tile_label, tile_label));
}

TEST_CASE("text that ends in three dots reads as cut", "[ui_redraw]")
{
    TEST_ASSERT_TRUE(ui_text_cut_with_dots("100% ..."));
    TEST_ASSERT_TRUE(ui_text_cut_with_dots("Claude 5h u..."));
    TEST_ASSERT_TRUE(ui_text_cut_with_dots("..."));
    TEST_ASSERT_FALSE(ui_text_cut_with_dots("42%"));
    TEST_ASSERT_FALSE(ui_text_cut_with_dots("wait.."));
    TEST_ASSERT_FALSE(ui_text_cut_with_dots(".."));
    TEST_ASSERT_FALSE(ui_text_cut_with_dots(""));
    TEST_ASSERT_FALSE(ui_text_cut_with_dots(NULL));
}

TEST_CASE("a label with no text takes the set", "[ui_redraw]")
{
    TEST_ASSERT_FALSE(ui_text_unchanged(NULL, ""));
    TEST_ASSERT_FALSE(ui_text_unchanged(NULL, "--"));
}
