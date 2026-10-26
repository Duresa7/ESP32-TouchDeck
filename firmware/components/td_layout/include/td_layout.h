#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "ui_pages.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The schema's limits, from schema/README.md. A stored layout file larger than TD_LAYOUT_MAX_BYTES is refused
 * before it is parsed. */
#define TD_LAYOUT_MAX_BYTES 32768
#define TD_LAYOUT_MAX_PAGES 8
#define TD_LAYOUT_MAX_TILES 64
#define TD_LAYOUT_MAX_DEPTH 16

/* The ids are at most 15 characters. A title or name is at most 24 code points, so 96 bytes of UTF-8. */
#define TD_LAYOUT_ID_MAX 16
#define TD_LAYOUT_TITLE_MAX 97
#define TD_LAYOUT_LABEL_MAX 65

/*
 * Why a layout was refused. `key` is the rule key from schema/README.md, the same key that schema/check-layouts.mjs
 * reports, so every file under schema/examples/invalid/ fails here with the key on its first line. The loader adds
 * `file.missing` and `file.read`, and `internal.no-memory` covers an allocation that failed. `message` names the
 * page and tile.
 */
typedef struct {
    char key[32];
    char message[160];
} td_layout_error_t;

/*
 * A loaded layout. One arena owns this header, the pages, the tiles and every string they point to, so LVGL's
 * static labels stay valid as long as the layout lives. Free it with td_layout_free(). The built-in layout is never
 * freed.
 */
typedef struct td_layout {
    const char *name;
    const char *orientation;
    const ui_page_t *pages;
    uint8_t page_count;
    uint16_t tile_count;
    uint16_t icons_wanted;
    size_t arena_bytes;
    bool builtin;
} td_layout_t;

/*
 * Parses and validates one layout file of `len` bytes, which need not end in a NUL. It accepts exactly the files
 * that schema/check-layouts.mjs accepts. On success *out is a new layout and the function returns true. On failure it
 * returns false, *out is NULL, and *err holds the first rule the file breaks. It never touches LVGL or storage, so the
 * unit tests run it as is.
 */
bool td_layout_parse(const char *json, size_t len, td_layout_t **out, td_layout_error_t *err);

/* Frees a parsed layout. NULL and the built-in layout are ignored. */
void td_layout_free(td_layout_t *layout);

/* schema/examples/builtin.json as the firmware embeds it. */
const char *td_layout_builtin_json(size_t *len);

/* The built-in layout, parsed on the first call. It lives for the whole boot. NULL only when the embedded file is
 * invalid, which the unit tests rule out. */
const td_layout_t *td_layout_builtin(void);

/* Reads and parses a stored layout. A file over TD_LAYOUT_MAX_BYTES is refused as `layout.too-large` before it is
 * parsed. *file_bytes is the file's size, or 0 when it could not be read. */
bool td_layout_load_file(const char *path, td_layout_t **out, size_t *file_bytes, td_layout_error_t *err);

/* "/storage/layouts/<orientation>.json". */
void td_layout_path(const char *orientation, char *buf, size_t cap);

/*
 * Makes `next` the current layout and returns the layout that the caller frees once nothing draws from it any more,
 * or NULL. The built-in layout is never returned, so replacing it frees nothing.
 */
td_layout_t *td_layout_replace(const td_layout_t **current, const td_layout_t *next);

/* The index of the page with this id, or -1. */
int td_layout_page_index(const td_layout_t *layout, const char *id);

/* The tile with this id on any page, with its page index in *page, or NULL. */
const ui_tile_t *td_layout_find_tile(const td_layout_t *layout, const char *id, uint8_t *page);

#ifdef __cplusplus
}
#endif
