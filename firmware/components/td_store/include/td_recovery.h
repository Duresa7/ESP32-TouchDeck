#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "esp_console.h"
#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif

#define TD_RECOVERY_MARKER_PATH "/storage/recovery"
/* From the lift of an armed contact to the restart, so the lift lands before the deck goes. */
#define TD_RECOVERY_RESTART_DELAY_MS 250

/*
 * Recovery mode is a boolean for one boot. The deck enters it when the boot finds the marker file, which it deletes
 * at once, and in it the deck runs the built-in pages and the default settings from RAM and leaves every file alone.
 * Arming the 10-second hold writes the marker and restarts. Nothing else keeps recovery across a boot.
 */

/* Reads the marker and deletes it. True, and recovery mode on, when it was there. Call it once, after td_store_mount()
 * and before anything reads the settings or the layout. A boot with no mounted volume is not in recovery mode. */
bool td_recovery_boot(void);

/* Whether this boot is in recovery mode. Any task may call it. */
bool td_recovery_mode(void);

/* Leaves recovery mode, which a deliberate write by the app or a lane does: layout.put, settings.put, or a
 * `layout use storage`. A no-op outside it. `why` goes in the log. */
void td_recovery_leave(const char *why);

/* Writes the marker file. */
esp_err_t td_recovery_mark(void);

/* The 10-second hold armed. Logs it, then writes the marker without blocking the caller, which is the LVGL task.
 * td_recovery_released() starts the restart. */
void td_recovery_armed(void);

/* The armed contact lifted. The deck restarts TD_RECOVERY_RESTART_DELAY_MS later, once the marker is down. */
void td_recovery_released(void);

/* What `recovery state` reports about the touch watcher, which lives in the touch component. */
typedef struct {
    const char *watcher; /* IDLE, HOLDING or ARMED */
    uint32_t held_ms;
    uint32_t armed_count;
} td_recovery_info_t;

typedef void (*td_recovery_info_fn_t)(td_recovery_info_t *out);

/* The touch component registers how to read its watcher, so this component never depends on it. */
void td_recovery_set_info_fn(td_recovery_info_fn_t fn);

/* The `recovery` console command, for td_console_register(). */
const esp_console_cmd_t *td_recovery_console_commands(size_t *count);

#ifdef __cplusplus
}
#endif
