#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define TD_SETTINGS_IDLE_DEFAULT_S 600
#define TD_SETTINGS_IDLE_MIN_S 1
#define TD_SETTINGS_IDLE_MAX_S 86400

/* Room for the longest settings file td_settings_format writes, with its NUL. */
#define TD_SETTINGS_JSON_MAX 80

typedef enum {
    TD_ORIENTATION_LANDSCAPE,
    TD_ORIENTATION_PORTRAIT,
} td_orientation_t;

typedef struct {
    uint32_t idle_timeout_s;
    td_orientation_t orientation;
} td_settings_t;

void td_settings_defaults(td_settings_t *out);

/* "landscape" or "portrait". */
const char *td_settings_orientation_name(td_orientation_t orientation);

/* True only for the two names above, exactly. */
bool td_settings_orientation_parse(const char *name, td_orientation_t *out);

/*
 * Reads a settings file. Never fails to fill *out: an unknown field is ignored, a missing field takes its default,
 * an idle_timeout_s that is not a whole number from 1 to 86400 takes the default, and an orientation that is not
 * "landscape" or "portrait" takes landscape. A file that is not a JSON object gives all defaults and false. True
 * when the file was a JSON object, even one that held nothing usable.
 */
bool td_settings_parse(const char *json, size_t len, td_settings_t *out);

/* Writes `{"idle_timeout_s":N,"orientation":"landscape"}` with a NUL and returns its length, or 0 when cap is too
 * small or the settings are out of range. */
size_t td_settings_format(const td_settings_t *settings, char *buf, size_t cap);

/*
 * Two pure helpers the store and its console commands share, kept here so the board's unit test can run them
 * without a mounted volume.
 *
 * td_store_path_valid: a path under /storage, at most 95 characters, whose segments hold only letters, digits,
 * '.', '_' and '-', never "." or "..", never empty, at most 32 characters each. With allow_root the bare
 * "/storage" is valid too.
 *
 * td_store_b64_decode: strict base64 with padding, as the @tds lines carry it. The length must be a multiple of
 * 4, '=' may only close the last group, and the unused bits must be zero, so one byte string has one encoding.
 * False for anything else or when the result would not fit in out_cap. *out_len is set only on success.
 */
bool td_store_path_valid(const char *path, bool allow_root);
bool td_store_b64_decode(const char *in, size_t in_len, uint8_t *out, size_t out_cap, size_t *out_len);

/* Standard base64 with padding and a NUL. Returns the characters written, not counting the NUL, or 0 when cap is
 * smaller than 4 * ceil(len / 3) + 1 or len is 0. */
size_t td_store_b64_encode(const uint8_t *in, size_t len, char *out, size_t cap);

#ifdef __cplusplus
}
#endif
