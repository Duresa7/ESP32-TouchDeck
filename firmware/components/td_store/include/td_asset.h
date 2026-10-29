#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Assets on the storage volume, keyed by the SHA-256 of their bytes, as schema/protocol/link-v2.md says. A committed
 * Asset is /storage/assets/<shard>/<id>.bin, where <id> is 64 lowercase hex digits and <shard> is its first digit.
 * An upload streams into
 * /storage/assets/upload.tmp while its SHA-256 is computed, and only a complete file whose hash and format check out is
 * renamed into place, so a cut upload never publishes a partial Asset. The mount removes a leftover upload.tmp.
 *
 * The id is longer than the 32-character segments td_store_path_valid allows, so this module builds its own fixed
 * paths and takes the store's lock itself. Every function runs on one task at a time: the link task.
 */

#define TD_ASSET_DIR "/storage/assets"
#define TD_ASSET_ID_LEN 64
/* A listing page holds at most this many Assets. */
#define TD_ASSET_PAGE 4
/* Room the volume keeps for Layouts, settings and LittleFS metadata. An upload that would cut into it is `full`. */
#define TD_ASSET_HEADROOM_BYTES 65536
/* The largest Asset this Firmware takes: one tdi1 icon, the only format it validates. */
#define TD_ASSET_MAX_BYTES 12300

typedef enum {
    TD_ASSET_OK,
    TD_ASSET_FULL,
    TD_ASSET_STORAGE,
    TD_ASSET_HASH,
    TD_ASSET_FORMAT,
    TD_ASSET_STATE,
    TD_ASSET_INCOMPLETE,
} td_asset_result_t;

typedef enum {
    TD_ASSET_FMT_NONE,
    TD_ASSET_FMT_TDI1,
} td_asset_format_t;

typedef struct {
    uint32_t total_bytes;
    uint32_t used_bytes;
    uint32_t reserved_bytes;
    uint32_t free_bytes;
} td_asset_space_t;

typedef struct {
    char id[TD_ASSET_ID_LEN + 1];
    uint32_t bytes;
    td_asset_format_t format;
} td_asset_item_t;

/* 64 lowercase hex digits. */
bool td_asset_id_valid(const char *id);

/* The wire name of a result, as `reason` carries it: "full", "storage", "hash", "format", "state", "incomplete". */
const char *td_asset_result_reason(td_asset_result_t result);

/* "tdi1" and TD_ASSET_FMT_TDI1 both ways. Any other name is TD_ASSET_FMT_NONE, and NONE has no name (NULL). */
td_asset_format_t td_asset_format_parse(const char *name);
const char *td_asset_format_name(td_asset_format_t format);

/* The volume's size and use, the reservation for headroom and an open upload, and what is left for Assets:
 * free = max(0, total - used - reserved). TD_ASSET_STORAGE when the volume is not mounted. */
td_asset_result_t td_asset_space(td_asset_space_t *out);

/* The bytes an Asset of this size may take on the volume, with its blocks and metadata, rounded up. */
uint32_t td_asset_cost(uint32_t bytes);

/* True when <id>.bin is committed and its bytes still hash to id and pass format validation. Fills bytes and format
 * when they are not NULL. */
bool td_asset_present(const char *id, uint32_t *bytes, td_asset_format_t *format);

/* Up to TD_ASSET_PAGE committed Assets with an id strictly after `after` (NULL for the start), ordered by id.
 * *more tells whether any remain after the last one. TD_ASSET_STORAGE when the volume is not mounted. */
td_asset_result_t td_asset_list(const char *after, td_asset_item_t items[TD_ASSET_PAGE], size_t *count, bool *more);

/* Removes <id>.bin. *deleted is false when it was not there, which is still TD_ASSET_OK. */
td_asset_result_t td_asset_delete(const char *id, bool *deleted);

/* Opens the upload of `bytes` bytes that must hash to id. TD_ASSET_FULL when td_asset_cost(bytes) exceeds the free
 * space, TD_ASSET_STATE when an upload is already open, TD_ASSET_STORAGE when the temporary file cannot be made. */
td_asset_result_t td_asset_upload_begin(const char *id, uint32_t bytes, td_asset_format_t format);

/* Appends to the open upload. More bytes than declared is TD_ASSET_STATE. A failed write discards the upload and
 * returns TD_ASSET_FULL when the volume ran out of space, otherwise TD_ASSET_STORAGE. */
td_asset_result_t td_asset_upload_write(const uint8_t *data, size_t len);

/* Commits the open upload. TD_ASSET_INCOMPLETE while bytes are missing, which keeps it open. Otherwise the upload
 * closes: TD_ASSET_HASH when the bytes do not hash to the id, TD_ASSET_FORMAT when they are not a valid file of the
 * declared format, TD_ASSET_FULL or TD_ASSET_STORAGE when the flush or rename fails, TD_ASSET_OK when <id>.bin now
 * holds the bytes. */
td_asset_result_t td_asset_upload_finish(void);

/* Discards the open upload and its temporary file. Does nothing when none is open. */
void td_asset_upload_abort(void);

/* The id of the open upload, or NULL. */
const char *td_asset_upload_id(void);

#ifdef __cplusplus
}
#endif
