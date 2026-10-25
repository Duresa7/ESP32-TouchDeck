#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "esp_console.h"
#include "esp_err.h"
#include "td_settings.h"

#ifdef __cplusplus
extern "C" {
#endif

#define TD_STORE_ROOT "/storage"
#define TD_STORE_SETTINGS_PATH "/storage/settings.json"
/* The longest path td_store_path_valid accepts, with its NUL. A write adds ".tmp" beside it. */
#define TD_STORE_PATH_MAX 96
/* `store put` refuses a file larger than this. The layout limit, 32768 bytes, sits well under it. */
#define TD_STORE_PUT_MAX 65536

typedef struct {
    uint32_t total_bytes;
    uint32_t used_bytes;
} td_store_info_t;

typedef struct {
    uint32_t size; /* 0 for a directory */
    bool is_dir;
} td_store_stat_t;

/* One call per directory entry, never "." or "..". Return false to stop the walk. The callback runs with the store's
 * lock held, so it must not call another td_store function from a different task. */
typedef bool (*td_store_list_cb_t)(void *ctx, const char *name, uint32_t size, bool is_dir);

/*
 * The one LittleFS volume at /storage, on the partition labelled "storage". A volume that will not mount is formatted
 * and mounted again, which is the first boot after the partition was erased. *formatted tells whether that happened.
 * Stale "<file>.tmp" files, left by a write that lost power before its rename, are removed from the volume. Call it
 * once, before anything else here. ESP_ERR_NOT_FOUND when the partition is missing, ESP_FAIL when it cannot be mounted
 * even after a format, ESP_ERR_INVALID_STATE on a second call.
 */
esp_err_t td_store_mount(bool *formatted);

bool td_store_mounted(void);

/* True when this boot had to format the volume. */
bool td_store_formatted(void);

/* The store's own recursive lock, for a caller that must keep several calls together. Every function here takes it. */
void td_store_lock(void);
void td_store_unlock(void);

/* ESP_ERR_INVALID_STATE before a successful mount. */
esp_err_t td_store_info(td_store_info_t *out);

/* ESP_ERR_INVALID_ARG for a path td_store_path_valid refuses, ESP_ERR_NOT_FOUND when nothing is there. */
esp_err_t td_store_stat(const char *path, td_store_stat_t *out);

/* dir may be "/storage". ESP_ERR_NOT_FOUND when it does not exist, ESP_ERR_INVALID_ARG when it is a file. */
esp_err_t td_store_list(const char *dir, td_store_list_cb_t cb, void *ctx);

/* Reads the whole file into buf. *out_len is the file's size, also when it is larger than max, which gives
 * ESP_ERR_INVALID_SIZE with buf untouched. ESP_ERR_NOT_FOUND when there is no such file, ESP_FAIL on a read error. */
esp_err_t td_store_read(const char *path, void *buf, size_t max, size_t *out_len);

#define TD_STORE_CHUNK_MAX 256

/* One call per chunk of the file, at most the requested size. Return false to stop. */
typedef bool (*td_store_chunk_cb_t)(void *ctx, const uint8_t *data, size_t len);

/* Streams the file in chunks of up to `chunk` bytes (1 to TD_STORE_CHUNK_MAX) with the store's lock held, so a write
 * cannot replace the file mid-stream. ESP_ERR_NOT_FOUND when there is no such file, ESP_ERR_INVALID_RESPONSE when
 * the callback stopped the stream, ESP_FAIL on a read error. */
esp_err_t td_store_read_chunks(const char *path, size_t chunk, td_store_chunk_cb_t cb, void *ctx);

/*
 * Writes <path>.tmp, fsyncs it, and renames it over path, creating the directories above it. LittleFS makes the rename
 * atomic across power loss, so a reader sees the whole old file or the whole new one, never a mix. On any error the
 * temporary file is removed and the old file is left as it was, and the result is ESP_FAIL. ESP_ERR_INVALID_ARG for
 * a path td_store_path_valid refuses or one ending in ".tmp", ESP_ERR_INVALID_STATE before a successful mount.
 */
esp_err_t td_store_write_atomic(const char *path, const void *data, size_t len);

/* A file or an empty directory. ESP_ERR_NOT_FOUND when nothing is there, ESP_FAIL otherwise. */
esp_err_t td_store_remove(const char *path);

/* Settings from /storage/settings.json, with the defaults for anything missing or unreadable, and the defaults for
 * all of it when the file does not exist. Always fills *out. Returns ESP_ERR_NOT_FOUND for an absent file, ESP_OK for
 * a file that parsed, ESP_ERR_INVALID_RESPONSE for one that was not a JSON object, and ESP_ERR_INVALID_STATE when
 * the volume is not mounted. */
esp_err_t td_store_settings_load(td_settings_t *out);

/* Formats and writes the settings atomically. ESP_ERR_INVALID_ARG for out-of-range values. */
esp_err_t td_store_settings_save(const td_settings_t *settings);

/* The `store`, `@tds` and `settings` commands, for td_console_register(). Before a mount, they answer "store not
 * mounted". */
const esp_console_cmd_t *td_store_console_commands(size_t *count);

#ifdef __cplusplus
}
#endif
