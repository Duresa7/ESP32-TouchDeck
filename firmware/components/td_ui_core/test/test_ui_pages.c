#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include "unity.h"
#include "ui_pages.h"

#define GAP 16
#define MAX_IDS 64

#define EXPECT_RECT(got, want_x, want_y, want_w, want_h) \
    do {                                                 \
        TEST_ASSERT_EQUAL_INT16((want_x), (got).x);      \
        TEST_ASSERT_EQUAL_INT16((want_y), (got).y);      \
        TEST_ASSERT_EQUAL_INT16((want_w), (got).w);      \
        TEST_ASSERT_EQUAL_INT16((want_h), (got).h);      \
    } while (0)

static const ui_rect_t k_area = { .x = 28, .y = 76, .w = 968, .h = 464 };

static const char *const k_fkeys_ids[] = {
    "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12", "shift", "ctrl", "alt", "win",
};
static const char *const k_fkeys_labels[] = {
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "Shift", "Ctrl", "Alt", "Win",
};
static const char *const k_media_ids[] = { "prev", "playpause", "next", "mute", "voldown", "volup" };
static const char *const k_media_labels[] = { "Prev", "Play/Pause", "Next", "Mute", "Vol -", "Vol +" };

static const ui_page_t *builtin_page(size_t index)
{
    size_t count = 0;
    const ui_page_t *pages = ui_builtin_pages(&count);
    TEST_ASSERT_TRUE(index < count);
    return &pages[index];
}

static ui_rect_t rect_of(const ui_page_t *page, const char *id)
{
    const ui_tile_t *tile = ui_page_find_tile(page, id);
    TEST_ASSERT_NOT_NULL_MESSAGE(tile, id);
    return ui_tile_rect(page, tile, k_area, GAP);
}

static void expect_tiles(const ui_page_t *page, const char *const *ids, const char *const *labels, size_t n,
                         uint8_t cols)
{
    TEST_ASSERT_EQUAL_UINT8(n, page->tile_count);
    for (size_t i = 0; i < n; i++) {
        const ui_tile_t *tile = &page->tiles[i];
        TEST_ASSERT_EQUAL_STRING(ids[i], tile->id);
        TEST_ASSERT_EQUAL_STRING(labels[i], tile->label);
        TEST_ASSERT_EQUAL_MESSAGE(i / cols, tile->row, ids[i]);
        TEST_ASSERT_EQUAL_MESSAGE(i % cols, tile->col, ids[i]);
        TEST_ASSERT_EQUAL_MESSAGE(1, tile->row_span, ids[i]);
        TEST_ASSERT_EQUAL_MESSAGE(1, tile->col_span, ids[i]);
    }
}

static bool rects_overlap(ui_rect_t a, ui_rect_t b)
{
    return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

static bool rows_share(const ui_tile_t *a, const ui_tile_t *b)
{
    return a->row < b->row + b->row_span && b->row < a->row + a->row_span;
}

static bool cols_share(const ui_tile_t *a, const ui_tile_t *b)
{
    return a->col < b->col + b->col_span && b->col < a->col + a->col_span;
}

static bool left_of(const ui_tile_t *a, const ui_tile_t *b)
{
    return a->col + a->col_span == b->col && rows_share(a, b);
}

static bool above(const ui_tile_t *a, const ui_tile_t *b)
{
    return a->row + a->row_span == b->row && cols_share(a, b);
}

/* Checks the gap between every pair of tiles that sit next to each other and returns how many pairs it found. */
static size_t check_neighbours(const ui_tile_t *a, ui_rect_t ra, const ui_tile_t *b, ui_rect_t rb)
{
    size_t found = 0;
    if (left_of(a, b)) {
        TEST_ASSERT_EQUAL_INT_MESSAGE(GAP, rb.x - (ra.x + ra.w), a->id);
        found++;
    }
    if (left_of(b, a)) {
        TEST_ASSERT_EQUAL_INT_MESSAGE(GAP, ra.x - (rb.x + rb.w), a->id);
        found++;
    }
    if (above(a, b)) {
        TEST_ASSERT_EQUAL_INT_MESSAGE(GAP, rb.y - (ra.y + ra.h), a->id);
        found++;
    }
    if (above(b, a)) {
        TEST_ASSERT_EQUAL_INT_MESSAGE(GAP, ra.y - (rb.y + rb.h), a->id);
        found++;
    }
    return found;
}

static void check_tile_in_area(const ui_page_t *page, const ui_tile_t *tile, ui_rect_t r)
{
    TEST_ASSERT_TRUE_MESSAGE(r.w > 0 && r.h > 0, tile->id);
    TEST_ASSERT_TRUE_MESSAGE(r.x >= k_area.x && r.y >= k_area.y, tile->id);
    TEST_ASSERT_TRUE_MESSAGE(r.x + r.w <= k_area.x + k_area.w && r.y + r.h <= k_area.y + k_area.h, tile->id);
    if (tile->col == 0) {
        TEST_ASSERT_EQUAL_INT_MESSAGE(k_area.x, r.x, tile->id);
    }
    if (tile->row == 0) {
        TEST_ASSERT_EQUAL_INT_MESSAGE(k_area.y, r.y, tile->id);
    }
    if (tile->col + tile->col_span == page->cols) {
        TEST_ASSERT_EQUAL_INT_MESSAGE(k_area.x + k_area.w, r.x + r.w, tile->id);
    }
    if (tile->row + tile->row_span == page->rows) {
        TEST_ASSERT_EQUAL_INT_MESSAGE(k_area.y + k_area.h, r.y + r.h, tile->id);
    }
}

/* Every built-in tile is 1 by 1, so within a page all the widths, and all the heights, are within a pixel. */
static size_t check_builtin_layout(const ui_page_t *page)
{
    size_t neighbours = 0;
    int min_w = INT16_MAX;
    int max_w = 0;
    int min_h = INT16_MAX;
    int max_h = 0;
    for (size_t i = 0; i < page->tile_count; i++) {
        const ui_tile_t *a = &page->tiles[i];
        const ui_rect_t ra = ui_tile_rect(page, a, k_area, GAP);
        check_tile_in_area(page, a, ra);
        min_w = ra.w < min_w ? ra.w : min_w;
        max_w = ra.w > max_w ? ra.w : max_w;
        min_h = ra.h < min_h ? ra.h : min_h;
        max_h = ra.h > max_h ? ra.h : max_h;

        for (size_t j = i + 1; j < page->tile_count; j++) {
            const ui_tile_t *b = &page->tiles[j];
            const ui_rect_t rb = ui_tile_rect(page, b, k_area, GAP);
            TEST_ASSERT_FALSE_MESSAGE(rects_overlap(ra, rb), a->id);
            neighbours += check_neighbours(a, ra, b, rb);
        }
    }
    TEST_ASSERT_TRUE_MESSAGE(max_w - min_w <= 1, page->id);
    TEST_ASSERT_TRUE_MESSAGE(max_h - min_h <= 1, page->id);
    return neighbours;
}

static ui_page_t page_of(uint8_t rows, uint8_t cols, const ui_tile_t *tiles, uint8_t count)
{
    return (ui_page_t){ .id = "p", .title = "P", .rows = rows, .cols = cols, .tile_count = count, .tiles = tiles };
}

static void expect_invalid(const char *why, uint8_t rows, uint8_t cols, const ui_tile_t *tiles, uint8_t count)
{
    const ui_page_t page = page_of(rows, cols, tiles, count);
    TEST_ASSERT_FALSE_MESSAGE(ui_page_valid(&page), why);
}

static void expect_valid(const char *why, uint8_t rows, uint8_t cols, const ui_tile_t *tiles, uint8_t count)
{
    const ui_page_t page = page_of(rows, cols, tiles, count);
    TEST_ASSERT_TRUE_MESSAGE(ui_page_valid(&page), why);
}

static void expect_empty_rect(const char *why, const ui_page_t *page, const ui_tile_t *tile)
{
    const ui_rect_t r = ui_tile_rect(page, tile, k_area, GAP);
    TEST_ASSERT_EQUAL_INT16_MESSAGE(0, r.w, why);
    TEST_ASSERT_EQUAL_INT16_MESSAGE(0, r.h, why);
}

TEST_CASE("the built-in table holds the F-Keys page then the Media page", "[ui_pages]")
{
    size_t count = 0;
    const ui_page_t *pages = ui_builtin_pages(&count);
    TEST_ASSERT_NOT_NULL(pages);
    TEST_ASSERT_EQUAL_UINT32(2, count);

    TEST_ASSERT_EQUAL_STRING("fkeys", pages[0].id);
    TEST_ASSERT_EQUAL_STRING("F-Keys", pages[0].title);
    TEST_ASSERT_EQUAL_UINT8(4, pages[0].rows);
    TEST_ASSERT_EQUAL_UINT8(4, pages[0].cols);
    TEST_ASSERT_EQUAL_STRING("media", pages[1].id);
    TEST_ASSERT_EQUAL_STRING("Media", pages[1].title);
    TEST_ASSERT_EQUAL_UINT8(2, pages[1].rows);
    TEST_ASSERT_EQUAL_UINT8(3, pages[1].cols);

    size_t again = 0;
    TEST_ASSERT_EQUAL_PTR_MESSAGE(pages, ui_builtin_pages(&again), "the table must be the same one every call");
    TEST_ASSERT_EQUAL_UINT32(2, again);
}

TEST_CASE("every built-in page passes ui_page_valid", "[ui_pages]")
{
    size_t count = 0;
    const ui_page_t *pages = ui_builtin_pages(&count);
    for (size_t i = 0; i < count; i++) {
        TEST_ASSERT_TRUE_MESSAGE(ui_page_valid(&pages[i]), pages[i].id);
    }
}

TEST_CASE("page ids and tile ids are non-empty lowercase ASCII and unique across the table", "[ui_pages]")
{
    size_t count = 0;
    const ui_page_t *pages = ui_builtin_pages(&count);

    const char *ids[MAX_IDS];
    size_t n = 0;
    for (size_t i = 0; i < count; i++) {
        TEST_ASSERT_TRUE(n < MAX_IDS);
        ids[n++] = pages[i].id;
        for (size_t j = 0; j < pages[i].tile_count; j++) {
            TEST_ASSERT_TRUE(n < MAX_IDS);
            ids[n++] = pages[i].tiles[j].id;
        }
    }
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(2 + 16 + 6, n, "2 pages, 16 F-Keys tiles and 6 Media tiles");

    for (size_t i = 0; i < n; i++) {
        TEST_ASSERT_NOT_NULL(ids[i]);
        TEST_ASSERT_TRUE_MESSAGE(ids[i][0] != '\0', "an id must not be empty");
        for (const char *c = ids[i]; *c != '\0'; c++) {
            const bool ok = (*c >= 'a' && *c <= 'z') || (*c >= '0' && *c <= '9');
            TEST_ASSERT_TRUE_MESSAGE(ok, ids[i]);
        }
        char message[48];
        snprintf(message, sizeof message, "duplicate id %s", ids[i]);
        for (size_t j = i + 1; j < n; j++) {
            TEST_ASSERT_TRUE_MESSAGE(strcmp(ids[i], ids[j]) != 0, message);
        }
    }
}

TEST_CASE("the F-Keys page has F1 to F12 on rows 0 to 2 and the modifiers on row 3", "[ui_pages]")
{
    expect_tiles(builtin_page(0), k_fkeys_ids, k_fkeys_labels, 16, 4);
}

TEST_CASE("the Media page has transport tiles on row 0 and volume tiles on row 1", "[ui_pages]")
{
    expect_tiles(builtin_page(1), k_media_ids, k_media_labels, 6, 3);
}

TEST_CASE("F-Keys tile rects follow from the deck's 968 by 464 grid with a 16 pixel gap", "[ui_pages]")
{
    const ui_page_t *page = builtin_page(0);
    /* Columns are 230 wide at x 28, 274, 520 and 766. Rows are 104 high at y 76, 196, 316 and 436. */
    EXPECT_RECT(rect_of(page, "f1"), 28, 76, 230, 104);
    EXPECT_RECT(rect_of(page, "f2"), 274, 76, 230, 104);
    EXPECT_RECT(rect_of(page, "f4"), 766, 76, 230, 104);
    EXPECT_RECT(rect_of(page, "f6"), 274, 196, 230, 104);
    EXPECT_RECT(rect_of(page, "f10"), 274, 316, 230, 104);
    EXPECT_RECT(rect_of(page, "f12"), 766, 316, 230, 104);
    EXPECT_RECT(rect_of(page, "shift"), 28, 436, 230, 104);
    EXPECT_RECT(rect_of(page, "win"), 766, 436, 230, 104);

    const ui_rect_t f4 = rect_of(page, "f4");
    TEST_ASSERT_EQUAL_INT_MESSAGE(996, f4.x + f4.w, "the last column ends on the area's right edge");
    const ui_rect_t win = rect_of(page, "win");
    TEST_ASSERT_EQUAL_INT_MESSAGE(540, win.y + win.h, "the last row ends on the area's bottom edge");
}

TEST_CASE("Media tile rects follow from the deck's 968 by 464 grid with a 16 pixel gap", "[ui_pages]")
{
    const ui_page_t *page = builtin_page(1);
    /* Columns are 312 wide at x 28, 356 and 684. Rows are 224 high at y 76 and 316. */
    EXPECT_RECT(rect_of(page, "prev"), 28, 76, 312, 224);
    EXPECT_RECT(rect_of(page, "playpause"), 356, 76, 312, 224);
    EXPECT_RECT(rect_of(page, "next"), 684, 76, 312, 224);
    EXPECT_RECT(rect_of(page, "mute"), 28, 316, 312, 224);
    EXPECT_RECT(rect_of(page, "voldown"), 356, 316, 312, 224);
    EXPECT_RECT(rect_of(page, "volup"), 684, 316, 312, 224);
}

TEST_CASE("built-in tiles stay in the area, never overlap and sit exactly one gap apart", "[ui_pages]")
{
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(12 + 12, check_builtin_layout(builtin_page(0)),
                                     "4 rows of 3 horizontal pairs and 3 rows of 4 vertical pairs");
    TEST_ASSERT_EQUAL_UINT32_MESSAGE(4 + 3, check_builtin_layout(builtin_page(1)),
                                     "2 rows of 2 horizontal pairs and 1 row of 3 vertical pairs");
}

TEST_CASE("a tile that spans cells covers them and the gaps between them", "[ui_pages]")
{
    static const ui_tile_t tiles[] = {
        { "big", "Big", 0, 0, 2, 2 },
        { "wide", "Wide", 3, 0, 1, 3 },
        { "tall", "Tall", 2, 3, 2, 1 },
    };
    const ui_page_t page = page_of(4, 4, tiles, 3);
    TEST_ASSERT_TRUE(ui_page_valid(&page));

    /* big is 230 + 16 + 230 wide and 104 + 16 + 104 high. wide is 3 * 230 + 2 * 16. tall is 104 + 16 + 104. */
    EXPECT_RECT(rect_of(&page, "big"), 28, 76, 476, 224);
    EXPECT_RECT(rect_of(&page, "wide"), 28, 436, 722, 104);
    EXPECT_RECT(rect_of(&page, "tall"), 766, 316, 230, 224);

    const ui_page_t *fkeys = builtin_page(0);
    const ui_rect_t f1 = rect_of(fkeys, "f1");
    const ui_rect_t f6 = rect_of(fkeys, "f6");
    const ui_rect_t big = rect_of(&page, "big");
    TEST_ASSERT_EQUAL_INT_MESSAGE(f1.x, big.x, "a spanning tile starts at its first cell");
    TEST_ASSERT_EQUAL_INT_MESSAGE(f1.y, big.y, "a spanning tile starts at its first cell");
    TEST_ASSERT_EQUAL_INT_MESSAGE(f6.x + f6.w, big.x + big.w, "a spanning tile ends at its last cell");
    TEST_ASSERT_EQUAL_INT_MESSAGE(f6.y + f6.h, big.y + big.h, "a spanning tile ends at its last cell");
}

TEST_CASE("an area that does not divide evenly keeps exact gaps and ends on its edge", "[ui_pages]")
{
    static const ui_tile_t tiles[] = {
        { "a", "A", 0, 0, 2, 2 },
        { "b", "B", 0, 2, 1, 1 },
        { "c", "C", 1, 2, 1, 1 },
        { "d", "D", 2, 0, 1, 3 },
    };
    const ui_page_t page = page_of(3, 3, tiles, 4);
    TEST_ASSERT_TRUE(ui_page_valid(&page));

    /* 100 + 10 divides in 3 at 36 and 73, so the cells run 0-26, 36-63 and 73-100 in each direction. */
    const ui_rect_t area = { .x = 0, .y = 0, .w = 100, .h = 100 };
    EXPECT_RECT(ui_tile_rect(&page, &tiles[0], area, 10), 0, 0, 63, 63);
    EXPECT_RECT(ui_tile_rect(&page, &tiles[1], area, 10), 73, 0, 27, 26);
    EXPECT_RECT(ui_tile_rect(&page, &tiles[2], area, 10), 73, 36, 27, 27);
    EXPECT_RECT(ui_tile_rect(&page, &tiles[3], area, 10), 0, 73, 100, 27);
}

TEST_CASE("a tile outside the grid or outside a valid grid gets an empty rect", "[ui_pages]")
{
    const ui_tile_t below = { "t", "T", 2, 0, 1, 1 };
    const ui_tile_t right = { "t", "T", 0, 2, 1, 1 };
    const ui_tile_t tall = { "t", "T", 1, 0, 2, 1 };
    const ui_tile_t wide = { "t", "T", 0, 1, 1, 2 };
    const ui_tile_t flat = { "t", "T", 0, 0, 0, 1 };
    const ui_tile_t thin = { "t", "T", 0, 0, 1, 0 };
    const ui_tile_t inside = { "t", "T", 0, 0, 1, 1 };

    const ui_page_t page = page_of(2, 2, &inside, 1);
    expect_empty_rect("a row below the grid", &page, &below);
    expect_empty_rect("a column right of the grid", &page, &right);
    expect_empty_rect("a row span past the bottom", &page, &tall);
    expect_empty_rect("a column span past the right edge", &page, &wide);
    expect_empty_rect("a zero row span", &page, &flat);
    expect_empty_rect("a zero column span", &page, &thin);

    const ui_page_t no_rows = page_of(0, 2, &inside, 1);
    const ui_page_t too_many_cols = page_of(2, UI_GRID_MAX + 1, &inside, 1);
    expect_empty_rect("a grid with no rows", &no_rows, &inside);
    expect_empty_rect("a grid wider than UI_GRID_MAX", &too_many_cols, &inside);
}

TEST_CASE("ui_page_valid accepts tiles that touch without sharing a cell", "[ui_pages]")
{
    expect_valid("two tiles side by side", 2, 2,
                 (const ui_tile_t[]){ { "a", "A", 0, 0, 1, 1 }, { "b", "B", 0, 1, 1, 1 } }, 2);
    expect_valid("a tall tile beside a short one", 2, 2,
                 (const ui_tile_t[]){ { "a", "A", 0, 0, 2, 1 }, { "b", "B", 0, 1, 1, 1 } }, 2);
    expect_valid("a tile in the last cell of the biggest grid", UI_GRID_MAX, UI_GRID_MAX,
                 (const ui_tile_t[]){ { "a", "A", UI_GRID_MAX - 1, UI_GRID_MAX - 1, 1, 1 } }, 1);
    expect_valid("a 1 by 1 grid", 1, 1, (const ui_tile_t[]){ { "a", "A", 0, 0, 1, 1 } }, 1);
}

TEST_CASE("ui_page_valid rejects tiles that share a cell", "[ui_pages]")
{
    expect_invalid("two tiles on one cell", 2, 2,
                   (const ui_tile_t[]){ { "a", "A", 0, 0, 1, 1 }, { "b", "B", 0, 0, 1, 1 } }, 2);
    expect_invalid("a spanning tile over a single one", 2, 2,
                   (const ui_tile_t[]){ { "a", "A", 0, 0, 2, 2 }, { "b", "B", 1, 1, 1, 1 } }, 2);
    expect_invalid("a single tile under a spanning one", 2, 3,
                   (const ui_tile_t[]){ { "a", "A", 0, 2, 1, 1 }, { "b", "B", 0, 0, 1, 3 } }, 2);
    expect_invalid("two spanning tiles that cross", 3, 3,
                   (const ui_tile_t[]){ { "a", "A", 0, 1, 3, 1 }, { "b", "B", 1, 0, 1, 3 } }, 2);
}

TEST_CASE("ui_page_valid rejects a tile outside the grid", "[ui_pages]")
{
    expect_invalid("a row below the grid", 2, 2, (const ui_tile_t[]){ { "a", "A", 2, 0, 1, 1 } }, 1);
    expect_invalid("a column right of the grid", 2, 2, (const ui_tile_t[]){ { "a", "A", 0, 2, 1, 1 } }, 1);
    expect_invalid("a row span past the bottom", 2, 2, (const ui_tile_t[]){ { "a", "A", 1, 0, 2, 1 } }, 1);
    expect_invalid("a column span past the right edge", 2, 2, (const ui_tile_t[]){ { "a", "A", 0, 1, 1, 2 } }, 1);
}

TEST_CASE("ui_page_valid rejects a zero span", "[ui_pages]")
{
    expect_invalid("a zero row span", 2, 2, (const ui_tile_t[]){ { "a", "A", 0, 0, 0, 1 } }, 1);
    expect_invalid("a zero column span", 2, 2, (const ui_tile_t[]){ { "a", "A", 0, 0, 1, 0 } }, 1);
}

TEST_CASE("ui_page_valid rejects a grid that is empty or bigger than UI_GRID_MAX", "[ui_pages]")
{
    const ui_tile_t tile[] = { { "a", "A", 0, 0, 1, 1 } };
    expect_invalid("zero rows", 0, 2, tile, 1);
    expect_invalid("zero cols", 2, 0, tile, 1);
    expect_invalid("zero rows and cols", 0, 0, tile, 1);
    expect_invalid("one row too many", UI_GRID_MAX + 1, 2, tile, 1);
    expect_invalid("one col too many", 2, UI_GRID_MAX + 1, tile, 1);
}

TEST_CASE("ui_page_valid rejects an empty or missing id or label", "[ui_pages]")
{
    expect_invalid("an empty tile id", 2, 2, (const ui_tile_t[]){ { "", "A", 0, 0, 1, 1 } }, 1);
    expect_invalid("a missing tile id", 2, 2, (const ui_tile_t[]){ { NULL, "A", 0, 0, 1, 1 } }, 1);
    expect_invalid("an empty tile label", 2, 2, (const ui_tile_t[]){ { "a", "", 0, 0, 1, 1 } }, 1);
    expect_invalid("a missing tile label", 2, 2, (const ui_tile_t[]){ { "a", NULL, 0, 0, 1, 1 } }, 1);
    expect_invalid("a bad tile after a good one", 2, 2,
                   (const ui_tile_t[]){ { "a", "A", 0, 0, 1, 1 }, { "b", "", 0, 1, 1, 1 } }, 2);

    const ui_tile_t tile[] = { { "a", "A", 0, 0, 1, 1 } };
    ui_page_t page = page_of(2, 2, tile, 1);
    page.id = "";
    TEST_ASSERT_FALSE_MESSAGE(ui_page_valid(&page), "an empty page id");

    page = page_of(2, 2, tile, 1);
    page.title = "";
    TEST_ASSERT_FALSE_MESSAGE(ui_page_valid(&page), "an empty page title");
    page.title = NULL;
    TEST_ASSERT_FALSE_MESSAGE(ui_page_valid(&page), "a missing page title");
}

TEST_CASE("ui_page_valid rejects a missing page and a tile list that is missing", "[ui_pages]")
{
    TEST_ASSERT_FALSE(ui_page_valid(NULL));
    expect_invalid("tiles counted but no table", 2, 2, NULL, 1);
    expect_valid("an empty page", 2, 2, NULL, 0);
}

TEST_CASE("ui_page_find_tile finds a tile by id and nothing else", "[ui_pages]")
{
    const ui_page_t *fkeys = builtin_page(0);
    const ui_page_t *media = builtin_page(1);

    TEST_ASSERT_EQUAL_PTR(&fkeys->tiles[0], ui_page_find_tile(fkeys, "f1"));
    TEST_ASSERT_EQUAL_PTR_MESSAGE(&fkeys->tiles[11], ui_page_find_tile(fkeys, "f12"), "f1 must not match f12");
    TEST_ASSERT_EQUAL_PTR(&fkeys->tiles[15], ui_page_find_tile(fkeys, "win"));
    TEST_ASSERT_EQUAL_PTR(&media->tiles[1], ui_page_find_tile(media, "playpause"));
    TEST_ASSERT_EQUAL_STRING("Vol +", ui_page_find_tile(media, "volup")->label);
}

TEST_CASE("ui_page_find_tile returns NULL for an unknown id, another page's id or a label", "[ui_pages]")
{
    const ui_page_t *fkeys = builtin_page(0);
    const ui_page_t *media = builtin_page(1);

    TEST_ASSERT_NULL(ui_page_find_tile(fkeys, "f13"));
    TEST_ASSERT_NULL(ui_page_find_tile(fkeys, ""));
    TEST_ASSERT_NULL_MESSAGE(ui_page_find_tile(fkeys, "prev"), "prev lives on the Media page");
    TEST_ASSERT_NULL_MESSAGE(ui_page_find_tile(media, "f1"), "f1 lives on the F-Keys page");
    TEST_ASSERT_NULL_MESSAGE(ui_page_find_tile(fkeys, "F1"), "an id is not a label");
    TEST_ASSERT_NULL_MESSAGE(ui_page_find_tile(media, "Mute"), "an id is not a label");
    TEST_ASSERT_NULL_MESSAGE(ui_page_find_tile(fkeys, "fkeys"), "a page id is not a tile id");
}
