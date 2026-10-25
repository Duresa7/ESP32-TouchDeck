#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define UI_GRID_MAX 8

/*
 * One tile on a page's grid. A tile covers row_span by col_span cells from its top-left cell, so milestone 2 can
 * load the same shape from flash. `id` is the stable name that the console and the layouts use. It is lowercase
 * ASCII and unique across every page. `label` is what the tile shows.
 */
typedef struct {
    const char *id;
    const char *label;
    uint8_t row;
    uint8_t col;
    uint8_t row_span;
    uint8_t col_span;
} ui_tile_t;

/* One swipeable page: a rows by cols grid and the tiles on it. `id` is unique across the pages. */
typedef struct {
    const char *id;
    const char *title;
    uint8_t rows;
    uint8_t cols;
    uint8_t tile_count;
    const ui_tile_t *tiles;
} ui_page_t;

typedef struct {
    int16_t x;
    int16_t y;
    int16_t w;
    int16_t h;
} ui_rect_t;

/* The built-in pages, in swipe order. The table is const and lives for good. */
const ui_page_t *ui_builtin_pages(size_t *count);

/*
 * True when the page is safe to draw. The grid is 1 to UI_GRID_MAX in each direction, the page's id and title
 * and every tile's id and label are non-empty, every tile has a span of at least 1 and stays inside the grid,
 * and no two tiles share a cell. It checks one page, so an id that two pages share is not caught here.
 */
bool ui_page_valid(const ui_page_t *page);

/*
 * Where a tile sits when the page's grid fills `area`, with `gap` pixels between neighbouring cells. Cell edges
 * come from integer division of the whole area, so every cell is within one pixel of the same size, the gaps are
 * exactly `gap`, and the last column and row end on the area's right and bottom edge. A spanning tile runs from
 * the first edge of its first cell to the last edge of its last cell, so it covers the gaps between its cells.
 * Returns an empty rect (w and h of 0) for a tile that does not fit a valid grid.
 */
ui_rect_t ui_tile_rect(const ui_page_t *page, const ui_tile_t *tile, ui_rect_t area, int16_t gap);

/* The tile with this id on this page, or NULL. */
const ui_tile_t *ui_page_find_tile(const ui_page_t *page, const char *id);

#ifdef __cplusplus
}
#endif
