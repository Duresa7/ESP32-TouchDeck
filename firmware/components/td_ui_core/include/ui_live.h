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
 * window. A Tile declares at most UI_LIVE_TILE_STREAMS streams, and one Layout at most UI_LIVE_SLOTS_MAX, of which at
 * most UI_LIVE_IMAGES_MAX are images. The images the slots hold and the ids waiting to be unpinned number at most
 * UI_LIVE_RELEASE_MAX together.
 */
#define UI_LIVE_SERIES_MAX 32
#define UI_LIVE_HZ 2
#define UI_LIVE_TOTAL_HZ 16
#define UI_LIVE_TILE_STREAMS 4
#define UI_LIVE_TEXT_MAX 128
#define UI_LIVE_IMAGES_MAX 4
#define UI_LIVE_IMAGE_ID_LEN 64
#define UI_LIVE_RELEASE_MAX (2 * UI_LIVE_IMAGES_MAX)
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
    UI_LIVE_ASSET,
} ui_live_result_t;

/* One `v2.live.set`, with its value already read from the JSON. `values` holds `count` numbers: one for a number,
 * the whole history, oldest first, for a series. `text` is a text value, and `image` an image value's `sha256:<id>`,
 * or NULL for null, which clears the image. */
typedef struct {
    const char *layout;
    const char *page;
    const char *tile;
    const char *stream;
    uint32_t n;
    uint32_t ttl_ms;
    ui_stream_type_t type;
    const double *values;
    const char *text;
    const char *image;
    size_t count;
} ui_live_set_t;

/*
 * One stream of one Live tile. A number or series stream keeps `samples`, a ring of `count` values that starts at
 * `head`; a number appends to it and drops the oldest past UI_LIVE_SERIES_MAX, and a series replaces it. A text stream
 * keeps `text`, and an image stream keeps in `image` the Asset id it shows, or "" for none. `set` is false until a
 * value arrives, so the Tile shows the unavailable marker. `stale` is true once the value outlived its ttl or the link
 * went offline; the value stays. `dirty` marks a slot the screen has not drawn since it changed.
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
    union {
        double samples[UI_LIVE_SERIES_MAX];
        char text[UI_LIVE_TEXT_MAX + 1];
        char image[UI_LIVE_IMAGE_ID_LEN + 1];
    };
} ui_live_slot_t;

/*
 * The live values of the Layout on screen. `slots` is owned by the caller and holds `cap` entries; ui_live_bind fills
 * the first `count`. `layout` is the SHA-256 of the active Layout's bytes in lowercase hex, or "builtin". Rate windows
 * are one second long and start at `session_ms`, the time protocol 2 was selected. `generation` counts every bind and
 * session, so a set that waited on a decode can tell its slot was replaced. `released` lists the Asset ids of image
 * values the table dropped, each holding a pin that ui_live_release gives back. The table is plain data with no lock,
 * so its owner guards it.
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
    uint32_t generation;
    uint8_t released_count;
    char released[UI_LIVE_RELEASE_MAX][UI_LIVE_IMAGE_ID_LEN + 1];
} ui_live_table_t;

/* The wire reason of a refusal: "layout", "page", "tile", "stream", "type", "args", "size", "sequence", "rate",
 * "memory" or "asset". NULL for UI_LIVE_OK. */
const char *ui_live_reason(ui_live_result_t result);

/* How many streams the Layout's Tiles declare, which is how many slots ui_live_bind wants. */
size_t ui_live_streams(const ui_page_t *pages, uint8_t page_count);

/*
 * Activates a Layout: a slot for each declared stream, in page and Tile order, up to `cap`, each with no value and no
 * counter. Streams past the cap, and image streams past UI_LIVE_IMAGES_MAX, count in `dropped`, and a set for one is
 * refused with UI_LIVE_MEMORY. `layout` is cut to UI_LIVE_LAYOUT_MAX bytes. The rate windows keep running. The images
 * of the old slots go to `released`.
 */
void ui_live_bind(ui_live_table_t *table, ui_live_slot_t *slots, uint16_t cap, const char *layout,
                  const ui_page_t *pages, uint8_t page_count);

/* A new protocol 2 selection: every value and counter is cleared, and the rate windows restart at now_ms. The images
 * the slots held go to `released`. */
void ui_live_session(ui_live_table_t *table, uint32_t now_ms);

/*
 * Applies one set and returns why it was refused, or UI_LIVE_OK. A refusal changes nothing. An identical retry of the
 * last accepted `n` succeeds without changing anything. *retry_ms is set for UI_LIVE_RATE: the time to the next
 * window, 1 to 1000. Text is at most UI_LIVE_TEXT_MAX bytes, and a bar's `value` is 0 to 1. An image is NULL or
 * `sha256:` and 64 lowercase hex digits; the image it replaces goes to `released`. A new image is refused with
 * UI_LIVE_MEMORY while the held images and `released` already fill UI_LIVE_RELEASE_MAX, so no pin is ever dropped,
 * and ui_live_release makes room again. This commits without checking the Asset, which is ui_live_apply's first phase.
 */
ui_live_result_t ui_live_set(ui_live_table_t *table, const ui_live_set_t *set, uint32_t now_ms, uint32_t *retry_ms);

/* Every check ui_live_set makes, without changing anything. *new_image is true when the set names an image its slot
 * does not hold yet, so the Asset must be pinned and decoded before the set can commit. */
ui_live_result_t ui_live_check(const ui_live_table_t *table, const ui_live_set_t *set, uint32_t now_ms,
                               uint32_t *retry_ms, bool *new_image);

/*
 * What ui_live_apply needs from the screen side. `lock` and `unlock` guard the table. `pin` holds an Asset against
 * deletion once it is checked to be a JPEG the Device draws, and answers UI_LIVE_OK, UI_LIVE_ASSET or UI_LIVE_MEMORY;
 * `unpin` gives it back. `decode` turns a pinned Asset into pixels, without the lock, and answers UI_LIVE_OK with
 * *pixels set, UI_LIVE_ASSET or UI_LIVE_MEMORY. `show` runs under the lock once a set commits: slot `index` now draws
 * `pixels`, or nothing for NULL, and it returns the pixels it replaced that never reached the screen. `discard` frees
 * pixels without the lock.
 */
typedef struct {
    void *ctx;
    void (*lock)(void *ctx);
    void (*unlock)(void *ctx);
    ui_live_result_t (*pin)(void *ctx, const char *id);
    void (*unpin)(void *ctx, const char *id);
    ui_live_result_t (*decode)(void *ctx, const char *id, void **pixels);
    void *(*show)(void *ctx, uint16_t index, void *pixels);
    void (*discard)(void *ctx, void *pixels);
} ui_live_ops_t;

/*
 * Applies one set in two phases, so no lock is held across a decode. It checks the set under the lock. A new image is
 * then pinned and decoded with the lock released, and the set commits under the lock again, where `show` hands the
 * pixels on. A refusal at either phase frees the decode and gives the pin back. A slot that was replaced while the
 * decode ran refuses with UI_LIVE_LAYOUT. It ends with ui_live_release.
 */
ui_live_result_t ui_live_apply(ui_live_table_t *table, const ui_live_set_t *set, uint32_t now_ms, uint32_t *retry_ms,
                               const ui_live_ops_t *ops);

/* Takes the `released` list under the lock and unpins each id without it. Call it after ui_live_bind and
 * ui_live_session, before the link handles its next message, so a delete of an old cover no longer answers in-use. */
void ui_live_release(ui_live_table_t *table, const ui_live_ops_t *ops);

/* Marks every value whose ttl ran out by now_ms stale, and dirty. True when any slot changed. */
bool ui_live_expire(ui_live_table_t *table, uint32_t now_ms);

/* Marks every value stale and dirty, because the link went offline. True when any slot changed. */
bool ui_live_offline(ui_live_table_t *table);

/* The slot of a Tile's stream, or NULL. */
ui_live_slot_t *ui_live_find(ui_live_table_t *table, const ui_tile_t *tile, const char *stream);

/* Copies a slot's samples to out, oldest first, and returns how many. */
uint8_t ui_live_samples(const ui_live_slot_t *slot, double out[UI_LIVE_SERIES_MAX]);

/* The newest sample of a number or series slot, or 0 when it has none. */
double ui_live_latest(const ui_live_slot_t *slot);

#ifdef __cplusplus
}
#endif
