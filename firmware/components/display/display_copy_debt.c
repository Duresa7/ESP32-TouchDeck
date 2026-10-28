#include "display_priv.h"
#include <stdatomic.h>
#include "esp_log.h"
#include "lvgl.h"
#include "lvgl_private.h"

/*
 * CONFIG_TD_PAY_COPY_DEBT. In the adapter's triple-partial mode each of the three panel frame buffers remembers where
 * it lags the newest frame, and the refresh that next draws into a buffer first copies those areas over from the front
 * buffer, PSRAM to PSRAM at about 20 MB/s. A refresh that redraws most of the screen, such as a page slide's frame or a
 * full redraw, leaves the two other buffers lagging by most of the screen, so each of the two refreshes after it
 * copies most of the screen, about 60 ms each. Without this option the first tap after a slide pays that.
 *
 * With it, a large refresh starts a wait. When the wait ends with LVGL idle, no area waiting to be drawn, no animation
 * and no pointer pressed, this invalidates one pixel that has not changed. LVGL draws it again the same, and the
 * adapter's copy for that refresh brings one lagging buffer up to date. After two such refreshes all three buffers
 * hold the same frame. Every refresh that is not large brings one buffer up to date, so a real one that comes first
 * saves an idle one, and every large refresh starts the wait over, so nothing is added while an animation runs.
 */

/* The buffers other than the one a large refresh drew into, each brought up to date by one refresh. */
#define DEBT_STEPS 2
/* A refresh is large when its areas add up to at least this share of the screen. The copy it leaves behind then
 * costs at least about 15 ms per buffer, twice a whole tap's refresh. A tile's glow is about 5 %. */
#define LARGE_PERCENT 25
/* The wait from a large refresh, or from a real refresh while debt is left, to the first step. A slide draws a frame
 * about every 60 ms, so twice that ends the wait only once the slide's last frame is out, and the debt is paid about
 * 250 ms after it, well inside the half second before a person taps again. */
#define SETTLE_MS 120
/* From one step's refresh to the next step. */
#define STEP_GAP_MS LV_DEF_REFR_PERIOD

static const char *TAG = "display";

static struct {
    lv_display_t *disp;
    lv_timer_t *timer;
    uint32_t large_px;
    uint8_t steps;
    bool own_refresh;
    atomic_uint paid;
} s;

static bool lvgl_idle(void)
{
    if (s.disp->inv_p > 0 || lv_anim_count_running() > 0) {
        return false;
    }
    for (lv_indev_t *indev = lv_indev_get_next(NULL); indev != NULL; indev = lv_indev_get_next(indev)) {
        if (lv_indev_get_type(indev) == LV_INDEV_TYPE_POINTER &&
            lv_indev_get_state(indev) == LV_INDEV_STATE_PRESSED) {
            return false;
        }
    }
    return true;
}

static void wait_for(uint32_t ms)
{
    lv_timer_set_period(s.timer, ms);
    lv_timer_reset(s.timer);
    lv_timer_resume(s.timer);
}

/* LVGL's timers run on the LVGL task, so this and on_flush_finish never run at once. */
static void pay_step(lv_timer_t *timer)
{
    if (!lvgl_idle()) {
        lv_timer_set_period(timer, SETTLE_MS);
        return;
    }
    /* The step's refresh restarts the timer, if another step is left, once it has flushed. */
    lv_timer_pause(timer);
    const lv_area_t pixel = { .x1 = 0, .y1 = 0, .x2 = 0, .y2 = 0 };
    lv_obj_invalidate_area(lv_display_get_screen_active(s.disp), &pixel);
    s.own_refresh = true;
    atomic_fetch_add(&s.paid, 1);
}

/* The last strip's flush still sees the refresh's joined areas, which LVGL clears once the refresh is done. */
static void on_flush_finish(lv_event_t *e)
{
    (void)e;
    if (!lv_display_flush_is_last(s.disp)) {
        return;
    }
    uint32_t px = 0;
    for (uint32_t i = 0; i < s.disp->inv_p; i++) {
        if (!s.disp->inv_area_joined[i]) {
            px += lv_area_get_size(&s.disp->inv_areas[i]);
        }
    }
    const bool own = s.own_refresh;
    s.own_refresh = false;
    if (px >= s.large_px) {
        s.steps = DEBT_STEPS;
        wait_for(SETTLE_MS);
    } else if (s.steps > 0 && --s.steps > 0) {
        wait_for(own ? STEP_GAP_MS : SETTLE_MS);
    } else {
        lv_timer_pause(s.timer);
    }
}

void display_copy_debt_attach(lv_display_t *disp)
{
    s.disp = disp;
    s.large_px = (uint32_t)lv_display_get_horizontal_resolution(disp) * lv_display_get_vertical_resolution(disp) *
                 LARGE_PERCENT / 100;
    s.timer = lv_timer_create(pay_step, SETTLE_MS, NULL);
    if (s.timer == NULL) {
        ESP_LOGE(TAG, "no timer, so the copy debt is left to the next refreshes");
        return;
    }
    lv_timer_pause(s.timer);
    lv_display_add_event_cb(disp, on_flush_finish, LV_EVENT_FLUSH_FINISH, NULL);
}

uint32_t display_copy_debt_paid(void)
{
    return atomic_load(&s.paid);
}
