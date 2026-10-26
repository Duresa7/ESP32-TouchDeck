#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "unity.h"
#include "td_layout.h"
#include "ui_geometry.h"
#include "ui_pages.h"

#define EXPECT_RECT(got, want_x, want_y, want_w, want_h) \
    do {                                                 \
        TEST_ASSERT_EQUAL_INT16((want_x), (got).x);      \
        TEST_ASSERT_EQUAL_INT16((want_y), (got).y);      \
        TEST_ASSERT_EQUAL_INT16((want_w), (got).w);      \
        TEST_ASSERT_EQUAL_INT16((want_h), (got).h);      \
    } while (0)

static bool rects_overlap(ui_rect_t a, ui_rect_t b)
{
    return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

static ui_geometry_t geometry_of(ui_orientation_t orientation)
{
    return ui_geometry(ui_screen_size(orientation));
}

TEST_CASE("landscape geometry reproduces the milestone 1 pixel values", "[ui_geometry]")
{
    const ui_geometry_t g = geometry_of(UI_LANDSCAPE);
    TEST_ASSERT_EQUAL_INT16(1024, g.screen.w);
    TEST_ASSERT_EQUAL_INT16(600, g.screen.h);
    EXPECT_RECT(g.band, 0, 56, 1024, 504);
    EXPECT_RECT(g.grid, 28, 76, 968, 464);
    TEST_ASSERT_EQUAL_INT16(16, g.gap);
    TEST_ASSERT_EQUAL_INT16(36, g.title_x);
    TEST_ASSERT_EQUAL_INT16(14, g.title_y);
    TEST_ASSERT_EQUAL_INT16(580, g.dot_y);
    TEST_ASSERT_EQUAL_INT16(34, g.dot_pitch);
    TEST_ASSERT_EQUAL_INT16(12, g.dot_size);
    TEST_ASSERT_EQUAL_INT16(11, g.dot_active_extra);
    TEST_ASSERT_EQUAL_INT16(48, g.bounce_px);
    TEST_ASSERT_EQUAL_INT16(16, g.slop);
    TEST_ASSERT_EQUAL_INT16(96, g.swipe);
    TEST_ASSERT_EQUAL_INT16(360, g.dash_value_x);
    TEST_ASSERT_EQUAL_INT16(600, g.dash_bar_x);
    TEST_ASSERT_EQUAL_INT16(495, ui_geometry_dot_x(&g, 0, 2));
    TEST_ASSERT_EQUAL_INT16(529, ui_geometry_dot_x(&g, 1, 2));
}

TEST_CASE("portrait geometry is 600 by 1024 with the band, grid and dots inside it", "[ui_geometry]")
{
    const ui_geometry_t g = geometry_of(UI_PORTRAIT);
    TEST_ASSERT_EQUAL_INT16(600, g.screen.w);
    TEST_ASSERT_EQUAL_INT16(1024, g.screen.h);
    EXPECT_RECT(g.band, 0, 56, 600, 928);
    EXPECT_RECT(g.grid, 28, 76, 544, 888);
    TEST_ASSERT_EQUAL_INT16(1004, g.dot_y);
    TEST_ASSERT_EQUAL_INT16(16, g.gap);
    TEST_ASSERT_EQUAL_INT16(96, g.swipe);
    TEST_ASSERT_TRUE(g.dash_value_x < g.dash_bar_x && g.dash_bar_x < g.grid.w);
    TEST_ASSERT_EQUAL_INT16(283, ui_geometry_dot_x(&g, 0, 2));
}

/* Every built-in tile in both orientations: inside the grid, inside the screen, and clear of every other tile on
 * its page. */
static void check_builtin_tiles(ui_orientation_t orientation)
{
    const ui_geometry_t g = geometry_of(orientation);
    const td_layout_t *layout = td_layout_builtin();
    TEST_ASSERT_NOT_NULL(layout);
    for (size_t p = 0; p < layout->page_count; p++) {
        const ui_page_t *page = &layout->pages[p];
        for (size_t i = 0; i < page->tile_count; i++) {
            const ui_rect_t r = ui_tile_rect(page, &page->tiles[i], g.grid, g.gap);
            TEST_ASSERT_TRUE_MESSAGE(r.w > 0 && r.h > 0, page->tiles[i].id);
            TEST_ASSERT_TRUE_MESSAGE(r.x >= g.grid.x && r.y >= g.grid.y, page->tiles[i].id);
            TEST_ASSERT_TRUE_MESSAGE(r.x + r.w <= g.grid.x + g.grid.w && r.y + r.h <= g.grid.y + g.grid.h,
                                     page->tiles[i].id);
            TEST_ASSERT_TRUE_MESSAGE(r.x + r.w <= g.screen.w && r.y + r.h <= g.screen.h, page->tiles[i].id);
            for (size_t j = 0; j < i; j++) {
                const ui_rect_t other = ui_tile_rect(page, &page->tiles[j], g.grid, g.gap);
                TEST_ASSERT_FALSE_MESSAGE(rects_overlap(r, other), page->tiles[i].id);
            }
        }
    }
}

TEST_CASE("every built-in tile fits the screen and overlaps no other tile in landscape", "[ui_geometry]")
{
    check_builtin_tiles(UI_LANDSCAPE);
}

TEST_CASE("every built-in tile fits the screen and overlaps no other tile in portrait", "[ui_geometry]")
{
    check_builtin_tiles(UI_PORTRAIT);
}

TEST_CASE("portrait lays the F-keys grid out 4 wide and 4 tall, taller than wide", "[ui_geometry]")
{
    const ui_geometry_t g = geometry_of(UI_PORTRAIT);
    const td_layout_t *layout = td_layout_builtin();
    TEST_ASSERT_NOT_NULL(layout);
    const ui_page_t *page = &layout->pages[0];
    TEST_ASSERT_EQUAL_UINT8(4, page->rows);
    TEST_ASSERT_EQUAL_UINT8(4, page->cols);
    const ui_rect_t f1 = ui_tile_rect(page, ui_page_find_tile(page, "f1"), g.grid, g.gap);
    const ui_rect_t win = ui_tile_rect(page, ui_page_find_tile(page, "win"), g.grid, g.gap);
    EXPECT_RECT(f1, 28, 76, 124, 210);
    TEST_ASSERT_EQUAL_INT16(g.grid.x + g.grid.w, win.x + win.w);
    TEST_ASSERT_EQUAL_INT16(g.grid.y + g.grid.h, win.y + win.h);
}

TEST_CASE("touch rotation maps the four panel corners to the screen's corners", "[ui_geometry]")
{
    static const struct {
        int32_t px, py;
        int16_t lx, ly;
        int16_t sx, sy;
    } corners[] = {
        /* panel point,  landscape,   portrait */
        { 0, 0, 0, 0, 0, 1023 },
        { 1023, 0, 1023, 0, 0, 0 },
        { 1023, 599, 1023, 599, 599, 0 },
        { 0, 599, 0, 599, 599, 1023 },
    };
    for (size_t i = 0; i < sizeof corners / sizeof corners[0]; i++) {
        const ui_point_t l = ui_touch_to_screen(UI_LANDSCAPE, corners[i].px, corners[i].py);
        TEST_ASSERT_EQUAL_INT16(corners[i].lx, l.x);
        TEST_ASSERT_EQUAL_INT16(corners[i].ly, l.y);
        const ui_point_t p = ui_touch_to_screen(UI_PORTRAIT, corners[i].px, corners[i].py);
        TEST_ASSERT_EQUAL_INT16(corners[i].sx, p.x);
        TEST_ASSERT_EQUAL_INT16(corners[i].sy, p.y);
    }
}

TEST_CASE("touch rotation clamps a point off the panel to its edge", "[ui_geometry]")
{
    const ui_point_t p = ui_touch_to_screen(UI_PORTRAIT, 2000, 900);
    TEST_ASSERT_EQUAL_INT16(599, p.x);
    TEST_ASSERT_EQUAL_INT16(0, p.y);
    const ui_point_t l = ui_touch_to_screen(UI_LANDSCAPE, -5, 700);
    TEST_ASSERT_EQUAL_INT16(0, l.x);
    TEST_ASSERT_EQUAL_INT16(599, l.y);
}
