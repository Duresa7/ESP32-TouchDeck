#include "touch_recovery.h"

static bool within_slop(const td_recovery_watch_t *w, td_touch_sample_t s)
{
    const int32_t dx = (int32_t)s.x - (int32_t)w->x0;
    const int32_t dy = (int32_t)s.y - (int32_t)w->y0;
    return dx * dx + dy * dy <= TD_RECOVERY_SLOP_PX * TD_RECOVERY_SLOP_PX;
}

td_recovery_step_t td_recovery_watch_step(td_recovery_watch_t *w, td_touch_sample_t sample, uint32_t now_ms)
{
    td_recovery_step_t step = { 0 };

    if (!sample.pressed) {
        step.swallow = w->state == TD_RECOVERY_ARMED;
        step.released = step.swallow;
        w->state = TD_RECOVERY_IDLE;
        w->spent = false;
        return step;
    }

    switch (w->state) {
    case TD_RECOVERY_IDLE:
        if (!w->spent) {
            *w = (td_recovery_watch_t){ .state = TD_RECOVERY_HOLDING, .x0 = sample.x, .y0 = sample.y,
                                        .t0_ms = now_ms, .armed_count = w->armed_count };
        }
        break;
    case TD_RECOVERY_HOLDING:
        if (!within_slop(w, sample)) {
            w->state = TD_RECOVERY_IDLE;
            w->spent = true;
            break;
        }
        w->held_ms = now_ms - w->t0_ms;
        if (w->held_ms >= TD_RECOVERY_HOLD_MS) {
            w->state = TD_RECOVERY_ARMED;
            w->armed_count++;
            step.armed = true;
            step.swallow = true;
        }
        break;
    case TD_RECOVERY_ARMED:
        w->held_ms = now_ms - w->t0_ms;
        step.swallow = true;
        break;
    }
    return step;
}

const char *td_recovery_state_name(td_recovery_state_t state)
{
    switch (state) {
    case TD_RECOVERY_IDLE:
        return "IDLE";
    case TD_RECOVERY_HOLDING:
        return "HOLDING";
    case TD_RECOVERY_ARMED:
        return "ARMED";
    }
    return "IDLE";
}
