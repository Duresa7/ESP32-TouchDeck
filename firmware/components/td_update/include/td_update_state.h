#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* An incremental SHA-256, so an update is hashed as its chunks arrive and no second pass over the slot is needed. */
typedef struct {
    uint32_t h[8];
    uint8_t block[64];
    uint64_t total;
    uint8_t fill;
} td_sha256_t;

#define TD_SHA256_BYTES 32
#define TD_SHA256_HEX 64

void td_sha256_init(td_sha256_t *s);
void td_sha256_update(td_sha256_t *s, const uint8_t *data, size_t len);
void td_sha256_final(td_sha256_t *s, uint8_t out[TD_SHA256_BYTES]);

/* Reads exactly 64 hex digits, either case, into 32 bytes. False for any other text. */
bool td_sha256_from_hex(const char *hex, uint8_t out[TD_SHA256_BYTES]);

/*
 * An update is a state machine. It holds no I/O: the caller drives esp_ota_begin, esp_ota_write, esp_ota_end and
 * esp_ota_set_boot_partition and tells the machine what happened, so the rules below run in the unit tests.
 *
 *   IDLE -> RECEIVING -> VERIFYING -> REBOOTING
 *              |             |
 *              +-> FAILED <--+      FAILED and IDLE accept a new begin; REBOOTING accepts nothing.
 *
 * A call the machine refuses returns a reason and changes nothing. A call that breaks a running update, such as a
 * wrong hash or a flash error, moves it to FAILED and records the reason.
 */
typedef enum {
    TD_UPDATE_IDLE,
    TD_UPDATE_RECEIVING,
    TD_UPDATE_VERIFYING,
    TD_UPDATE_REBOOTING,
    TD_UPDATE_FAILED,
} td_update_phase_t;

typedef enum {
    TD_UPDATE_OK,
    TD_UPDATE_E_BUSY,        /* an update is receiving, verifying or restarting */
    TD_UPDATE_E_STATE,       /* no update is receiving */
    TD_UPDATE_E_SIZE,        /* zero bytes, or more than the slot holds */
    TD_UPDATE_E_SHA_FORMAT,  /* the expected hash is not 64 hex digits */
    TD_UPDATE_E_RANGE,       /* a chunk past the declared size */
    TD_UPDATE_E_INCOMPLETE,  /* fewer bytes than declared when the update ends */
    TD_UPDATE_E_SHA,         /* the hash of the written bytes is not the expected one */
    TD_UPDATE_E_FLASH,       /* esp_ota_begin, write, end or set_boot_partition failed */
    TD_UPDATE_E_UNCONFIRMED, /* the running image has not confirmed itself yet */
    TD_UPDATE_E_PROJECT,     /* the image is for another project */
    TD_UPDATE_E_CUT,         /* the transfer stopped moving */
} td_update_reason_t;

typedef struct {
    td_update_phase_t phase;
    td_update_reason_t reason; /* what failed, while phase is FAILED */
    uint32_t handle;           /* the esp_ota handle, while RECEIVING */
    uint32_t bytes;            /* declared size */
    uint32_t written;
    td_sha256_t sha;
    uint8_t expected[TD_SHA256_BYTES];
} td_update_state_t;

void td_update_state_init(td_update_state_t *u);

/* IDLE or FAILED to RECEIVING. `slot_size` is the size of the partition the image goes to. An image that has not
 * confirmed itself yet is the only copy the bootloader can fall back on, so it refuses an update meanwhile. */
td_update_reason_t td_update_state_begin(td_update_state_t *u, uint32_t bytes, const char *sha256_hex,
                                         uint32_t slot_size, bool running_confirmed);

/* The flash accepted the begin, and handed back this handle. */
void td_update_state_started(td_update_state_t *u, uint32_t handle);

/* Counts one chunk of the image and hashes it. The caller writes the same bytes to flash. */
td_update_reason_t td_update_state_chunk(td_update_state_t *u, const uint8_t *data, size_t len);

/* RECEIVING to VERIFYING when every byte arrived and the hash matches. Otherwise FAILED. */
td_update_reason_t td_update_state_finish(td_update_state_t *u);

/* VERIFYING to REBOOTING, once the image is checked and the boot slot is set. */
void td_update_state_rebooting(td_update_state_t *u);

/* A running update to FAILED with a reason. */
void td_update_state_fail(td_update_state_t *u, td_update_reason_t reason);

/* A transfer that was cut. RECEIVING or VERIFYING to IDLE, and nothing else changes. */
void td_update_state_abort(td_update_state_t *u);

const char *td_update_phase_name(td_update_phase_t phase);
/* The short word a reply puts in `reason`, and the sentence it puts in `error`. */
const char *td_update_reason_name(td_update_reason_t reason);
const char *td_update_reason_text(td_update_reason_t reason);

/*
 * The image confirms itself only when the deck has shown its first frame, loaded a layout, and stayed up long
 * enough that a crash on the way would already have happened. A reset before that leaves the bootloader to mark
 * the slot ABORTED and boot the previous one.
 */
#define TD_UPDATE_CONFIRM_UPTIME_MS 10000

typedef struct {
    bool first_frame;
    bool layout_loaded;
    uint32_t uptime_ms;
} td_update_confirm_inputs_t;

bool td_update_confirm_ready(const td_update_confirm_inputs_t *in);

#ifdef __cplusplus
}
#endif
