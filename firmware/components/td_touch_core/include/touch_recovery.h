#pragma once

#include <stdbool.h>
#include <stdint.h>
#include "touch_queue.h"

#ifdef __cplusplus
extern "C" {
#endif

#define TD_RECOVERY_HOLD_MS 10000
#define TD_RECOVERY_SLOP_PX 24

typedef enum {
    TD_RECOVERY_IDLE,
    TD_RECOVERY_HOLDING,
    TD_RECOVERY_ARMED,
} td_recovery_state_t;

/*
 * The recovery gesture: one contact that stays within TD_RECOVERY_SLOP_PX of its first sample for
 * TD_RECOVERY_HOLD_MS. A zeroed watcher is idle and ready. Nothing here locks, so the owner serializes the
 * callers.
 */
typedef struct {
    td_recovery_state_t state;
    uint16_t x0;
    uint16_t y0;
    uint32_t t0_ms;
    uint32_t held_ms;
    uint32_t armed_count;
    /* A contact that moved past the slop stays out until it lifts, so one that drifted never arms later. */
    bool spent;
} td_recovery_watch_t;

typedef struct {
    /* This sample is not for the UI: it belongs to a contact that has armed, its closing release included. */
    bool swallow;
    /* This sample is the one that armed the watcher. */
    bool armed;
    /* This sample is the release that ends an armed contact. */
    bool released;
} td_recovery_step_t;

/*
 * Folds one raw touch sample into the watcher. `now_ms` is a millisecond clock that only has to be monotonic,
 * and may wrap.
 * - A press from idle starts a hold at that point, unless the contact already moved past the slop.
 * - A pressed sample within the slop of the first one keeps the hold, and arms it once the hold has lasted
 *   TD_RECOVERY_HOLD_MS. A pressed sample past the slop drops the hold, and the contact cannot start another.
 * - A release before arming returns to idle.
 * - An armed contact swallows every sample, and its release returns to idle with `released` set.
 * `held_ms` follows the hold while it runs and keeps the last value after it ends.
 */
td_recovery_step_t td_recovery_watch_step(td_recovery_watch_t *w, td_touch_sample_t sample, uint32_t now_ms);

const char *td_recovery_state_name(td_recovery_state_t state);

#ifdef __cplusplus
}
#endif
