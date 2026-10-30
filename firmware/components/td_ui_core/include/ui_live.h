#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "ui_pages.h"

#ifdef __cplusplus
extern "C" {
#endif

/*
 * The live-value limits this Firmware advertises in `caps`, from schema/protocol/link-v2.md. A graph keeps
 * UI_LIVE_SERIES_MAX samples. A stream takes UI_LIVE_HZ updates and the Device UI_LIVE_TOTAL_HZ in each one-second
 * window. A Tile declares at most UI_LIVE_TILE_STREAMS streams, and one Layout at most UI_LIVE_SLOTS_MAX.
 */
#define UI_LIVE_SERIES_MAX 32
#define UI_LIVE_HZ 2
#define UI_LIVE_TOTAL_HZ 16
#define UI_LIVE_TILE_STREAMS 1
#define UI_LIVE_SLOTS_MAX 64
#define UI_LIVE_TTL_MIN_MS 1000
#define UI_LIVE_TTL_MAX_MS 60000
#define UI_LIVE_VALUE_MAX 1e9
#define UI_LIVE_LAYOUT_MAX 71

/* Why a set was refused, in the order the checks run. ui_live_reason() gives the wire `reason`. */
typedef enum {
    UI_LIVE_OK = 0,
    UI_LIVE_LAYOUT,
    UI_LIVE_PAGE,
    UI_LIVE_TILE,
    UI_LIVE_STREAM,
    UI_LIVE_TYPE,
    UI_LIVE_ARGS,
    UI_LIVE_SIZE,
    UI_LIVE_SEQUENCE,
    UI_LIVE_RATE,
    UI_LIVE_MEMORY,
} ui_live_result_t;

/* One `v2.live.set`, with its value already read from the JSON. `values` holds `count` numbers: one for a number,
 * the whole history, oldest first, for a series. */
typedef struct {
    const char *layout;
    const char *page;
    const char *tile;
    const char *stream;
    uint32_t n;
    uint32_t ttl_ms;
    ui_stream_type_t type;
    const double *values;
    size_t count;
} ui_live_set_t;

/*
 * One stream of one Live tile. `samples` is a ring of `count` values that starts at `head`; a number appends to it
 * and drops the oldest past UI_LIVE_SERIES_MAX, and a series replaces it. `set` is false until a value arrives, so the
 * Tile shows the unavailable marker. `stale` is true once the value outlived its ttl or the link went offline; the
 * value stays. `dirty` marks a slot the screen has not drawn since it changed.
 */
typedef struct {
    uint8_t page;
    const ui_tile_t *tile;
    const ui_stream_t *stream;
    uint32_t n;
    uint32_t digest;
    bool set;
    bool stale;
    bool dirty;
    uint32_t expires_ms;
    uint32_t window;
    uint8_t in_window;
    uint8_t count;
    uint8_t head;
    double samples[UI_LIVE_SERIES_MAX];
} ui_live_slot_t;

/*
 * The live values of the Layout on screen. `slots` is owned by the caller and holds `cap` entries; ui_live_bind fills
 * the first `count`. `layout` is the SHA-256 of the active Layout's bytes in lowercase hex, or "builtin". Rate windows
 * are one second long and start at `session_ms`, the time protocol 2 was selected. The table is plain data with no
 * lock, so its owner guards it.
 */
typedef struct {
    ui_live_slot_t *slots;
    uint16_t count;
    uint16_t cap;
    uint16_t dropped;
    char layout[UI_LIVE_LAYOUT_MAX + 1];
    const ui_page_t *pages;
    uint8_t page_count;
    uint32_t session_ms;
    uint32_t window;
    uint8_t in_window;
} ui_live_table_t;

/* The wire reason of a refusal: "layout", "page", "tile", "stream", "type", "args", "size", "sequence", "rate" or
 * "memory". NULL for UI_LIVE_OK. */
const char *ui_live_reason(ui_live_result_t result);

/* How many streams the Layout's Tiles declare, which is how many slots ui_live_bind wants. */
size_t ui_live_streams(const ui_page_t *pages, uint8_t page_count);

/*
 * Activates a Layout: a slot for each declared stream, in page and Tile order, up to `cap`, each with no value and no
 * counter. Streams past the cap count in `dropped`, and a set for one is refused with UI_LIVE_MEMORY. `layout` is cut
 * to UI_LIVE_LAYOUT_MAX bytes. The rate windows keep running.
 */
void ui_live_bind(ui_live_table_t *table, ui_live_slot_t *slots, uint16_t cap, const char *layout,
                  const ui_page_t *pages, uint8_t page_count);

/* A new protocol 2 selection: every value and counter is cleared, and the rate windows restart at now_ms. */
void ui_live_session(ui_live_table_t *table, uint32_t now_ms);

/*
 * Applies one set and returns why it was refused, or UI_LIVE_OK. A refusal changes nothing. An identical retry of the
 * last accepted `n` succeeds without changing anything. *retry_ms is set for UI_LIVE_RATE: the time to the next
 * window, 1 to 1000.
 */
ui_live_result_t ui_live_set(ui_live_table_t *table, const ui_live_set_t *set, uint32_t now_ms, uint32_t *retry_ms);

/* Marks every value whose ttl ran out by now_ms stale, and dirty. True when any slot changed. */
bool ui_live_expire(ui_live_table_t *table, uint32_t now_ms);

/* Marks every value stale and dirty, because the link went offline. True when any slot changed. */
bool ui_live_offline(ui_live_table_t *table);

/* The slot of a Tile's stream, or NULL. */
ui_live_slot_t *ui_live_find(ui_live_table_t *table, const ui_tile_t *tile, const char *stream);

/* Copies a slot's samples to out, oldest first, and returns how many. */
uint8_t ui_live_samples(const ui_live_slot_t *slot, double out[UI_LIVE_SERIES_MAX]);

#ifdef __cplusplus
}
#endif
