#include "display.h"
#include "display_perf.h"
#include "display_priv.h"
#include <stdatomic.h>
#include "esp_attr.h"
#include "esp_ipc.h"
#include "esp_lcd_panel_ops.h"
#include "esp_lcd_panel_rgb.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "lvgl.h"
#include "lvgl_private.h"

/*
 * Test-only performance measurement, built with CONFIG_TD_PERF_TOOLS. The LVGL task and the panel interrupt
 * only bump counters and store time stamps here, and the console reads differences of them, so a measured window
 * costs a few atomic adds a frame and allocates and logs nothing.
 *
 * drawn      LVGL refreshes that flushed, display.c's frame counter.
 * shown      panel refreshes that scanned out a frame no earlier refresh had, so new frames on the glass. The adapter
 *            hands each finished frame to the panel with esp_lcd_panel_draw_bitmap, which points the driver at that
 *            frame buffer, and the wrapper below numbers those hand-overs. With bounce buffers the driver picks the
 *            frame buffer for the next scan-out in its frame-complete interrupt, just before it calls
 *            on_frame_buf_complete, so the chained callback compares the newest hand-over with the one the previous
 *            refresh picked. A frame replaced before any refresh picked it counts in drawn and never in shown. If the
 *            interrupt lands between the driver's buffer switch and the wrapper's count, a few instructions, that
 *            frame counts one refresh late.
 * refreshes  panel scan-outs, one on_frame_buf_complete each, about 24 a second.
 * render     LVGL's REFR_START to the FLUSH_START of the refresh's last area: layout and drawing, and the copies of
 *            the refresh's other areas, since LVGL draws in strips and flushes each before it draws the next.
 * render_only
 *            render less those copies, so layout and drawing alone.
 * flush      that FLUSH_START to its FLUSH_FINISH: the adapter's copy into the frame buffer, the hand-over and its wait
 *            for a free buffer. The adapter calls lv_display_flush_ready inside the flush callback, so FLUSH_FINISH is
 *            flush ready.
 * copy       the sum over all of a refresh's strips of FLUSH_START to FLUSH_FINISH: the adapter's copy of each strip
 *            into the PSRAM back buffer, and for the last strip all of flush.
 * px         pixels flushed, the sum of the strips' areas.
 * cpu        per core, 100 % less the idle task's share of FreeRTOS run time, which ticks on esp_timer. Interrupts
 *            count toward the task they interrupt, so the bounce buffer copy shows as idle on an idle core.
 *
 * A `perf tap` probe takes the time LVGL took the injected sample, then the end of the first refresh that flushed
 * after it, then the panel refresh that picked that frame, which starts its scan-out within about a millisecond.
 */

#if !CONFIG_FREERTOS_RUN_TIME_STATS_USING_ESP_TIMER
#error "the perf tools read FreeRTOS run time as microseconds, so it must tick on esp_timer"
#endif

static struct {
    lv_display_t *disp;
    display_perf_config_t config;
    _Atomic(display_perf_tap_fn) tap;
    esp_lcd_rgb_panel_frame_buf_complete_cb_t adapter_frame_cb;
    esp_lcd_panel_handle_t panel;
    uint32_t latched_seq;
    uint32_t refr_start_us;
    uint32_t flush_start_us;
    uint32_t strip_start_us;
    /* The refresh in progress, kept by the LVGL task alone. */
    uint32_t frame_render_us;
    uint32_t frame_copy_us;
    uint32_t frame_nonlast_us;
    uint32_t frame_strips;
    uint32_t frame_px;
    atomic_uint submit_seq;
    atomic_uint refreshes;
    atomic_uint shown;
    atomic_uint render_n;
    atomic_uint render_us;
    atomic_uint render_only_us;
    atomic_uint flush_n;
    atomic_uint flush_us;
    atomic_uint copy_us;
    atomic_ullong px;
    atomic_bool slot_on[DISPLAY_PERF_SLOTS];
    atomic_uint render_max[DISPLAY_PERF_SLOTS];
    atomic_uint render_only_max[DISPLAY_PERF_SLOTS];
    atomic_uint flush_max[DISPLAY_PERF_SLOTS];
    atomic_uint copy_max[DISPLAY_PERF_SLOTS];
} s;

/* Each field is written before the flag bit that publishes it, and only by the context that sets that bit. */
static struct {
    atomic_uint flags;
    uint32_t sample_us;
    uint32_t target_seq;
    uint32_t flush_us;
    uint32_t shown_us;
    uint32_t render_us;
    uint32_t copy_us;
    uint32_t strips;
    uint32_t px;
} s_probe[DISPLAY_PERF_PROBES];

static inline uint32_t now_us(void)
{
    return (uint32_t)esp_timer_get_time();
}

/* Moves a probe on when it has every bit of `need` and not yet `set`, and stores `value` in `field` first. */
static inline void probe_step(size_t i, uint32_t need, uint32_t set, uint32_t *field, uint32_t value)
{
    unsigned flags = atomic_load(&s_probe[i].flags);
    do {
        if ((flags & (need | set)) != need) {
            return;
        }
        *field = value;
    } while (!atomic_compare_exchange_weak(&s_probe[i].flags, &flags, flags | set));
}

void display_perf_note_panel(uint32_t pclk_hz, uint32_t bounce_lines, size_t num_fbs)
{
    s.config.pclk_hz = pclk_hz;
    s.config.bounce_lines = bounce_lines;
    s.config.num_fbs = (uint32_t)num_fbs;
}

void display_perf_note_mode(int tear_mode, int rotation)
{
    s.config.tear_mode = tear_mode;
    s.config.rotation = rotation;
}

void display_perf_note_handle(esp_lcd_panel_handle_t panel)
{
    s.panel = panel;
}

const display_perf_config_t *display_perf_config(void)
{
    return &s.config;
}

esp_lcd_panel_handle_t display_perf_panel(void)
{
    return s.panel;
}

lv_display_t *display_perf_display(void)
{
    return s.disp;
}

uint32_t display_perf_submits(void)
{
    return atomic_load(&s.submit_seq);
}

bool display_perf_redraw_pending(void)
{
    return s.disp != NULL && s.disp->inv_p > 0;
}

void display_perf_set_tap(display_perf_tap_fn tap)
{
    atomic_store(&s.tap, tap);
}

display_perf_tap_fn display_perf_tap(void)
{
    return atomic_load(&s.tap);
}

void display_perf_input(bool pressed)
{
    const size_t i = pressed ? DISPLAY_PERF_PROBE_PRESS : DISPLAY_PERF_PROBE_RELEASE;
    probe_step(i, DISPLAY_PERF_PROBE_ARMED, DISPLAY_PERF_PROBE_SAMPLED, &s_probe[i].sample_us, now_us());
}

void display_perf_probe_arm(void)
{
    for (size_t i = 0; i < DISPLAY_PERF_PROBES; i++) {
        atomic_store(&s_probe[i].flags, DISPLAY_PERF_PROBE_ARMED);
    }
}

void display_perf_probe_read(display_perf_probe_t *probes)
{
    for (size_t i = 0; i < DISPLAY_PERF_PROBES; i++) {
        probes[i] = (display_perf_probe_t){ .flags = atomic_load(&s_probe[i].flags),
                                            .sample_us = s_probe[i].sample_us,
                                            .flush_us = s_probe[i].flush_us,
                                            .shown_us = s_probe[i].shown_us,
                                            .render_us = s_probe[i].render_us,
                                            .copy_us = s_probe[i].copy_us,
                                            .strips = s_probe[i].strips,
                                            .px = s_probe[i].px };
    }
}

void display_perf_probe_disarm(void)
{
    for (size_t i = 0; i < DISPLAY_PERF_PROBES; i++) {
        atomic_store(&s_probe[i].flags, 0);
    }
}

/* Only the LVGL task records, so a plain compare and store keeps each running maximum. A figure that shares another's
 * count passes n NULL and is recorded first. */
static void record(atomic_uint *n, atomic_uint *sum, atomic_uint *max, uint32_t us)
{
    atomic_fetch_add(sum, us);
    if (n != NULL) {
        atomic_fetch_add(n, 1);
    }
    for (size_t slot = 0; slot < DISPLAY_PERF_SLOTS; slot++) {
        if (atomic_load(&s.slot_on[slot]) && us > atomic_load(&max[slot])) {
            atomic_store(&max[slot], us);
        }
    }
}

/* Moves a submitted probe on to flushed, with the figures of the refresh that flushed. */
static void probe_flushed(size_t i, uint32_t now)
{
    unsigned flags = atomic_load(&s_probe[i].flags);
    do {
        if ((flags & (DISPLAY_PERF_PROBE_SUBMITTED | DISPLAY_PERF_PROBE_FLUSHED)) != DISPLAY_PERF_PROBE_SUBMITTED) {
            return;
        }
        s_probe[i].flush_us = now;
        s_probe[i].render_us = s.frame_render_us;
        s_probe[i].copy_us = s.frame_copy_us;
        s_probe[i].strips = s.frame_strips;
        s_probe[i].px = s.frame_px;
    } while (!atomic_compare_exchange_weak(&s_probe[i].flags, &flags, flags | DISPLAY_PERF_PROBE_FLUSHED));
}

static void on_refr_start(lv_event_t *e)
{
    (void)e;
    s.refr_start_us = now_us();
    s.frame_copy_us = 0;
    s.frame_nonlast_us = 0;
    s.frame_strips = 0;
    s.frame_px = 0;
}

/* LVGL sends FLUSH_START and FLUSH_FINISH around every strip's flush callback, the last strip's included. */
static void on_flush_start(lv_event_t *e)
{
    const lv_area_t *area = lv_event_get_param(e);
    const uint32_t now = now_us();
    s.strip_start_us = now;
    s.frame_strips++;
    s.frame_px += area != NULL ? lv_area_get_size(area) : 0;
    if (!lv_display_flush_is_last(s.disp)) {
        return;
    }
    s.flush_start_us = now;
    s.frame_render_us = now - s.refr_start_us;
    record(NULL, &s.render_only_us, s.render_only_max, s.frame_render_us - s.frame_nonlast_us);
    record(&s.render_n, &s.render_us, s.render_max, s.frame_render_us);
}

static void on_flush_finish(lv_event_t *e)
{
    (void)e;
    const uint32_t now = now_us();
    const uint32_t strip_us = now - s.strip_start_us;
    s.frame_copy_us += strip_us;
    if (!lv_display_flush_is_last(s.disp)) {
        s.frame_nonlast_us += strip_us;
        return;
    }
    atomic_fetch_add(&s.px, s.frame_px);
    record(NULL, &s.copy_us, s.copy_max, s.frame_copy_us);
    record(&s.flush_n, &s.flush_us, s.flush_max, now - s.flush_start_us);
    for (size_t i = 0; i < DISPLAY_PERF_PROBES; i++) {
        probe_flushed(i, now);
    }
}

void display_perf_attach(lv_display_t *disp)
{
    s.disp = disp;
    lv_display_add_event_cb(disp, on_refr_start, LV_EVENT_REFR_START, NULL);
    lv_display_add_event_cb(disp, on_flush_start, LV_EVENT_FLUSH_START, NULL);
    lv_display_add_event_cb(disp, on_flush_finish, LV_EVENT_FLUSH_FINISH, NULL);
}

/*
 * The linker sends the adapter's calls to these two esp_lcd functions here (see CMakeLists.txt) and the real ones
 * still run, so the adapter behaves as before. The adapter registers its RGB callbacks once, and this keeps them all
 * and its context, and only puts the chained frame-complete callback in front of its own.
 */
esp_err_t __real_esp_lcd_rgb_panel_register_event_callbacks(esp_lcd_panel_handle_t panel,
                                                            const esp_lcd_rgb_panel_event_callbacks_t *callbacks,
                                                            void *user_ctx);
esp_err_t __wrap_esp_lcd_rgb_panel_register_event_callbacks(esp_lcd_panel_handle_t panel,
                                                            const esp_lcd_rgb_panel_event_callbacks_t *callbacks,
                                                            void *user_ctx);
esp_err_t __real_esp_lcd_panel_draw_bitmap(esp_lcd_panel_handle_t panel, int x_start, int y_start, int x_end,
                                           int y_end, const void *color_data);
esp_err_t __wrap_esp_lcd_panel_draw_bitmap(esp_lcd_panel_handle_t panel, int x_start, int y_start, int x_end,
                                           int y_end, const void *color_data);

static bool IRAM_ATTR on_frame_buf_complete(esp_lcd_panel_handle_t panel, const esp_lcd_rgb_panel_event_data_t *edata,
                                            void *user_ctx)
{
    atomic_fetch_add(&s.refreshes, 1);
    const uint32_t seq = atomic_load(&s.submit_seq);
    if (seq != s.latched_seq) {
        s.latched_seq = seq;
        atomic_fetch_add(&s.shown, 1);
        const uint32_t now = now_us();
        for (size_t i = 0; i < DISPLAY_PERF_PROBES; i++) {
            unsigned flags = atomic_load(&s_probe[i].flags);
            do {
                const uint32_t need = DISPLAY_PERF_PROBE_SUBMITTED;
                if ((flags & (need | DISPLAY_PERF_PROBE_SHOWN)) != need ||
                    (int32_t)(seq - s_probe[i].target_seq) < 0) {
                    break;
                }
                s_probe[i].shown_us = now;
            } while (!atomic_compare_exchange_weak(&s_probe[i].flags, &flags, flags | DISPLAY_PERF_PROBE_SHOWN));
        }
    }
    const esp_lcd_rgb_panel_frame_buf_complete_cb_t adapter = s.adapter_frame_cb;
    return adapter != NULL && adapter(panel, edata, user_ctx);
}

esp_err_t __wrap_esp_lcd_rgb_panel_register_event_callbacks(esp_lcd_panel_handle_t panel,
                                                            const esp_lcd_rgb_panel_event_callbacks_t *callbacks,
                                                            void *user_ctx)
{
    if (callbacks == NULL) {
        return __real_esp_lcd_rgb_panel_register_event_callbacks(panel, callbacks, user_ctx);
    }
    esp_lcd_rgb_panel_event_callbacks_t chained = *callbacks;
    s.adapter_frame_cb = callbacks->on_frame_buf_complete;
    chained.on_frame_buf_complete = on_frame_buf_complete;
    return __real_esp_lcd_rgb_panel_register_event_callbacks(panel, &chained, user_ctx);
}

/* The RGB panel is the only one that draws bitmaps, and only the LVGL task hands it frames. */
esp_err_t __wrap_esp_lcd_panel_draw_bitmap(esp_lcd_panel_handle_t panel, int x_start, int y_start, int x_end,
                                           int y_end, const void *color_data)
{
    const esp_err_t err = __real_esp_lcd_panel_draw_bitmap(panel, x_start, y_start, x_end, y_end, color_data);
    if (err == ESP_OK) {
        const uint32_t seq = atomic_fetch_add(&s.submit_seq, 1) + 1;
        for (size_t i = 0; i < DISPLAY_PERF_PROBES; i++) {
            probe_step(i, DISPLAY_PERF_PROBE_SAMPLED, DISPLAY_PERF_PROBE_SUBMITTED, &s_probe[i].target_seq, seq);
        }
    }
    return err;
}

/* Runs on the core it reads, in the IPC task, so that core's idle task has just been switched out and its run time
 * is current. */
static void read_core(void *arg)
{
    display_perf_core_t *core = arg;
    core->idle_us = ulTaskGetIdleRunTimeCounterForCore(xPortGetCoreID());
    core->t_us = esp_timer_get_time();
}

void display_perf_sample_cores(display_perf_core_t *cores)
{
    for (int core = 0; core < DISPLAY_PERF_CORES; core++) {
        cores[core] = (display_perf_core_t){ 0 };
        esp_ipc_call_blocking(core, read_core, &cores[core]);
    }
}

int32_t display_perf_busy_x10(const display_perf_core_t *from, const display_perf_core_t *to)
{
    const int64_t span = to->t_us - from->t_us;
    if (from->t_us == 0 || to->t_us == 0 || span <= 0) {
        return -1;
    }
    const uint64_t idle = (configRUN_TIME_COUNTER_TYPE)(to->idle_us - from->idle_us);
    const int64_t busy = 1000 - (int64_t)(idle * 1000 / (uint64_t)span);
    return (int32_t)(busy < 0 ? 0 : busy > 1000 ? 1000 : busy);
}

void display_perf_window_begin(display_perf_slot_t slot, display_perf_mark_t *mark)
{
    atomic_store(&s.render_max[slot], 0);
    atomic_store(&s.render_only_max[slot], 0);
    atomic_store(&s.flush_max[slot], 0);
    atomic_store(&s.copy_max[slot], 0);
    atomic_store(&s.slot_on[slot], true);
    display_perf_sample_cores(mark->cores);
    mark->drawn = display_frame_count();
    mark->shown = atomic_load(&s.shown);
    mark->refreshes = atomic_load(&s.refreshes);
    mark->render_n = atomic_load(&s.render_n);
    mark->render_us = atomic_load(&s.render_us);
    mark->render_only_us = atomic_load(&s.render_only_us);
    mark->flush_n = atomic_load(&s.flush_n);
    mark->flush_us = atomic_load(&s.flush_us);
    mark->copy_us = atomic_load(&s.copy_us);
    mark->px = atomic_load(&s.px);
    mark->t_us = esp_timer_get_time();
}

void display_perf_window_end(display_perf_slot_t slot, const display_perf_mark_t *mark, display_perf_window_t *out)
{
    const int64_t t_us = esp_timer_get_time();
    const uint32_t drawn = display_frame_count();
    const uint32_t shown = atomic_load(&s.shown);
    const uint32_t refreshes = atomic_load(&s.refreshes);
    const uint32_t render_n = atomic_load(&s.render_n) - mark->render_n;
    const uint32_t render_us = atomic_load(&s.render_us) - mark->render_us;
    const uint32_t render_only_us = atomic_load(&s.render_only_us) - mark->render_only_us;
    const uint32_t flush_n = atomic_load(&s.flush_n) - mark->flush_n;
    const uint32_t flush_us = atomic_load(&s.flush_us) - mark->flush_us;
    const uint32_t copy_us = atomic_load(&s.copy_us) - mark->copy_us;
    const uint64_t px = atomic_load(&s.px) - mark->px;
    atomic_store(&s.slot_on[slot], false);
    display_perf_core_t cores[DISPLAY_PERF_CORES];
    display_perf_sample_cores(cores);

    *out = (display_perf_window_t){
        .elapsed_ms = (uint32_t)((t_us - mark->t_us) / 1000),
        .drawn = drawn - mark->drawn,
        .shown = shown - mark->shown,
        .refreshes = refreshes - mark->refreshes,
        .render_n = render_n,
        .render_us_avg = render_n ? render_us / render_n : 0,
        .render_us_max = render_n ? atomic_load(&s.render_max[slot]) : 0,
        .render_only_us_avg = render_n ? render_only_us / render_n : 0,
        .render_only_us_max = render_n ? atomic_load(&s.render_only_max[slot]) : 0,
        .flush_n = flush_n,
        .copy_us_avg = flush_n ? copy_us / flush_n : 0,
        .copy_us_max = flush_n ? atomic_load(&s.copy_max[slot]) : 0,
        .flush_us_avg = flush_n ? flush_us / flush_n : 0,
        .flush_us_max = flush_n ? atomic_load(&s.flush_max[slot]) : 0,
        .px = px,
    };
    for (int core = 0; core < DISPLAY_PERF_CORES; core++) {
        out->cpu_x10[core] = display_perf_busy_x10(&mark->cores[core], &cores[core]);
    }
}
