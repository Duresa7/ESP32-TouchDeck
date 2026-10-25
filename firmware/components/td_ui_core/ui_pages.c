#include <string.h>
#include "ui_pages.h"

#define ARRAY_LEN(a) (sizeof(a) / sizeof((a)[0]))

static const ui_tile_t k_fkeys_tiles[] = {
    { "f1", "F1", 0, 0, 1, 1 },
    { "f2", "F2", 0, 1, 1, 1 },
    { "f3", "F3", 0, 2, 1, 1 },
    { "f4", "F4", 0, 3, 1, 1 },
    { "f5", "F5", 1, 0, 1, 1 },
    { "f6", "F6", 1, 1, 1, 1 },
    { "f7", "F7", 1, 2, 1, 1 },
    { "f8", "F8", 1, 3, 1, 1 },
    { "f9", "F9", 2, 0, 1, 1 },
    { "f10", "F10", 2, 1, 1, 1 },
    { "f11", "F11", 2, 2, 1, 1 },
    { "f12", "F12", 2, 3, 1, 1 },
    { "shift", "Shift", 3, 0, 1, 1 },
    { "ctrl", "Ctrl", 3, 1, 1, 1 },
    { "alt", "Alt", 3, 2, 1, 1 },
    { "win", "Win", 3, 3, 1, 1 },
};

static const ui_tile_t k_media_tiles[] = {
    { "prev", "Prev", 0, 0, 1, 1 },
    { "playpause", "Play/Pause", 0, 1, 1, 1 },
    { "next", "Next", 0, 2, 1, 1 },
    { "mute", "Mute", 1, 0, 1, 1 },
    { "voldown", "Vol -", 1, 1, 1, 1 },
    { "volup", "Vol +", 1, 2, 1, 1 },
};

static const ui_page_t k_pages[] = {
    {
        .id = "fkeys",
        .title = "F-Keys",
        .rows = 4,
        .cols = 4,
        .tile_count = ARRAY_LEN(k_fkeys_tiles),
        .tiles = k_fkeys_tiles,
    },
    {
        .id = "media",
        .title = "Media",
        .rows = 2,
        .cols = 3,
        .tile_count = ARRAY_LEN(k_media_tiles),
        .tiles = k_media_tiles,
    },
};

const ui_page_t *ui_builtin_pages(size_t *count)
{
    if (count != NULL) {
        *count = ARRAY_LEN(k_pages);
    }
    return k_pages;
}

static bool has_text(const char *s)
{
    return s != NULL && s[0] != '\0';
}

static bool grid_size_ok(uint8_t n)
{
    return n >= 1 && n <= UI_GRID_MAX;
}

static bool tile_fits(const ui_page_t *page, const ui_tile_t *tile)
{
    return grid_size_ok(page->rows) && grid_size_ok(page->cols) && tile->row_span >= 1 && tile->col_span >= 1 &&
           tile->row + tile->row_span <= page->rows && tile->col + tile->col_span <= page->cols;
}

static bool tiles_overlap(const ui_tile_t *a, const ui_tile_t *b)
{
    return a->row < b->row + b->row_span && b->row < a->row + a->row_span && a->col < b->col + b->col_span &&
           b->col < a->col + a->col_span;
}

bool ui_page_valid(const ui_page_t *page)
{
    if (page == NULL || !has_text(page->id) || !has_text(page->title) || !grid_size_ok(page->rows) ||
        !grid_size_ok(page->cols)) {
        return false;
    }
    if (page->tile_count > 0 && page->tiles == NULL) {
        return false;
    }
    for (size_t i = 0; i < page->tile_count; i++) {
        const ui_tile_t *tile = &page->tiles[i];
        if (!has_text(tile->id) || !has_text(tile->label) || !tile_fits(page, tile)) {
            return false;
        }
        for (size_t j = 0; j < i; j++) {
            if (tiles_overlap(tile, &page->tiles[j])) {
                return false;
            }
        }
    }
    return true;
}

/* The n-th cell edge from the area's start, counting each cell with the gap that follows it. */
static int32_t cell_edge(int32_t length, int32_t gap, int32_t cells, int32_t n)
{
    return n * (length + gap) / cells;
}

ui_rect_t ui_tile_rect(const ui_page_t *page, const ui_tile_t *tile, ui_rect_t area, int16_t gap)
{
    if (page == NULL || tile == NULL || !tile_fits(page, tile)) {
        return (ui_rect_t){ 0, 0, 0, 0 };
    }

    const int32_t x0 = cell_edge(area.w, gap, page->cols, tile->col);
    const int32_t x1 = cell_edge(area.w, gap, page->cols, tile->col + tile->col_span) - gap;
    const int32_t y0 = cell_edge(area.h, gap, page->rows, tile->row);
    const int32_t y1 = cell_edge(area.h, gap, page->rows, tile->row + tile->row_span) - gap;
    return (ui_rect_t){
        .x = (int16_t)(area.x + x0),
        .y = (int16_t)(area.y + y0),
        .w = (int16_t)(x1 - x0),
        .h = (int16_t)(y1 - y0),
    };
}

const ui_tile_t *ui_page_find_tile(const ui_page_t *page, const char *id)
{
    if (page == NULL || id == NULL) {
        return NULL;
    }
    for (size_t i = 0; i < page->tile_count; i++) {
        if (strcmp(page->tiles[i].id, id) == 0) {
            return &page->tiles[i];
        }
    }
    return NULL;
}
