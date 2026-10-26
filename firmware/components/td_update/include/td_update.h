#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "esp_console.h"
#include "esp_err.h"
#include "td_update_state.h"

#ifdef __cplusplus
extern "C" {
#endif

/* What the deck must have done before the running image may confirm itself. Both run on the confirm task. */
typedef bool (*td_update_ready_fn)(void);

/*
 * Starts the confirm watch. A new image boots as PENDING_VERIFY, and this confirms it with
 * esp_ota_mark_app_valid_cancel_rollback() once `first_frame` and `layout_loaded` are true and the deck has been up for
 * TD_UPDATE_CONFIRM_UPTIME_MS, then logs `update: confirmed <slot>`. A reset before that leaves the bootloader to mark
 * the slot ABORTED and boot the other one. An image that is not pending, such as one flashed over USB, is confirmed
 * at once. Call it once from app_main.
 */
esp_err_t td_update_start(td_update_ready_fn first_frame, td_update_ready_fn layout_loaded);

/* True once the running image is confirmed, or never awaited confirmation. */
bool td_update_confirmed(void);

/* Confirms the running image now, whatever the deck has done. The `update confirm` test command calls it. False when
 * the bootloader's record would not take the mark. */
bool td_update_confirm_early(void);

/*
 * The write side of an update over the link. All of these run on the link task. begin opens the slot the running image
 * does not occupy, and names it in *slot. write takes the image in order. end checks the hash, validates the image,
 * sets it as the boot slot and restarts the deck 250 ms later, and names the slot in *slot. A failure leaves the boot
 * slot alone and returns the reason.
 */
td_update_reason_t td_update_begin(uint32_t bytes, const char *sha256_hex, const char **slot);
bool td_update_write(const uint8_t *data, size_t len);
td_update_reason_t td_update_end(const char **slot);

/* Drops an update that is still receiving, and returns the machine to IDLE. */
void td_update_cancel(void);

/* The state machine as the console and the link report it. Any task may call it. */
typedef struct {
    td_update_phase_t phase;
    td_update_reason_t reason;
    uint32_t bytes;
    uint32_t written;
} td_update_view_t;

void td_update_view(td_update_view_t *out);

/* The `update` console command: state, confirm, invalidate. */
const esp_console_cmd_t *td_update_console_commands(size_t *count);

#ifdef __cplusplus
}
#endif
