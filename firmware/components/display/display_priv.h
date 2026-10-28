#pragma once

#include <stdbool.h>
#include <stdint.h>
#include "esp_err.h"
#include "esp_lcd_types.h"
#include "sdkconfig.h"
#include "td_shot.h"

#if CONFIG_TD_PERF_TOOLS
#include "display_perf.h"
#include "lvgl.h"
#include "td_console.h"

/* The perf tools' measurement, in display_perf.c, display_perf_cmds.c, display_perf_fb.c and display_perf_mem.c. See
 * the comment at the top of display_perf.c for what each figure counts. */

#define DISPLAY_PERF_CORES CONFIG_FREERTOS_NUMBER_OF_CORES

/* `perf fps` and `perf window` can run at once, so each keeps its own maxima. */
typedef enum {
    DISPLAY_PERF_SLOT_FPS,
    DISPLAY_PERF_SLOT_WINDOW,
    DISPLAY_PERF_SLOTS,
} display_perf_slot_t;

/* One core's clock and its idle task's run time, both in microseconds. t_us is 0 when the core could not be read. */
typedef struct {
    int64_t t_us;
    uint64_t idle_us;
} display_perf_core_t;

typedef struct {
    uint32_t drawn;
    uint32_t shown;
    uint32_t refreshes;
    uint32_t render_n;
    uint32_t render_us;
    uint32_t render_only_us;
    uint32_t flush_n;
    uint32_t flush_us;
    uint32_t copy_us;
    uint64_t px;
    int64_t t_us;
    display_perf_core_t cores[DISPLAY_PERF_CORES];
} display_perf_mark_t;

/* Averages and maxima are 0 when render_n or flush_n is, cpu_x10 is -1 when a core could not be read. */
typedef struct {
    uint32_t elapsed_ms;
    uint32_t drawn;
    uint32_t shown;
    uint32_t refreshes;
    uint32_t render_n;
    uint32_t render_us_avg;
    uint32_t render_us_max;
    uint32_t render_only_us_avg;
    uint32_t render_only_us_max;
    uint32_t flush_n;
    uint32_t copy_us_avg;
    uint32_t copy_us_max;
    uint32_t flush_us_avg;
    uint32_t flush_us_max;
    uint64_t px;
    int32_t cpu_x10[DISPLAY_PERF_CORES];
} display_perf_window_t;

/* What display_start and panel_rgb_create configured, for `perf info`. */
typedef struct {
    uint32_t pclk_hz;
    uint32_t bounce_lines;
    uint32_t num_fbs;
    int tear_mode;
    int rotation;
} display_perf_config_t;

/* `perf tap` follows the press and the release of one tap. */
enum {
    DISPLAY_PERF_PROBE_PRESS,
    DISPLAY_PERF_PROBE_RELEASE,
    DISPLAY_PERF_PROBES,
};

#define DISPLAY_PERF_PROBE_ARMED 0x01u
#define DISPLAY_PERF_PROBE_SAMPLED 0x02u
#define DISPLAY_PERF_PROBE_SUBMITTED 0x04u
#define DISPLAY_PERF_PROBE_FLUSHED 0x08u
#define DISPLAY_PERF_PROBE_SHOWN 0x10u

/* Time stamps are the low 32 bits of esp_timer_get_time(), so only their differences mean anything. render_us,
 * copy_us, strips and px describe the refresh that flushed, and are set with FLUSHED. */
typedef struct {
    uint32_t flags;
    uint32_t sample_us;
    uint32_t flush_us;
    uint32_t shown_us;
    uint32_t render_us;
    uint32_t copy_us;
    uint32_t strips;
    uint32_t px;
} display_perf_probe_t;

void display_perf_note_panel(uint32_t pclk_hz, uint32_t bounce_lines, size_t num_fbs);
void display_perf_note_mode(int tear_mode, int rotation);
void display_perf_note_handle(esp_lcd_panel_handle_t panel);
const display_perf_config_t *display_perf_config(void);
esp_lcd_panel_handle_t display_perf_panel(void);
lv_display_t *display_perf_display(void);

/* Frames the adapter has handed to the panel since boot. */
uint32_t display_perf_submits(void);

/* True while LVGL holds invalidated areas it has not drawn yet. Call it with the adapter lock held. */
bool display_perf_redraw_pending(void);

/* Adds the render and flush timers to the display. Call it with the adapter lock held. */
void display_perf_attach(lv_display_t *disp);

void display_perf_window_begin(display_perf_slot_t slot, display_perf_mark_t *mark);
void display_perf_window_end(display_perf_slot_t slot, const display_perf_mark_t *mark, display_perf_window_t *out);

/* Reads each core's idle run time from that core, so an idle task that is running gets its time so far counted. */
void display_perf_sample_cores(display_perf_core_t *cores);
int32_t display_perf_busy_x10(const display_perf_core_t *from, const display_perf_core_t *to);

void display_perf_probe_arm(void);
void display_perf_probe_read(display_perf_probe_t *probes);
void display_perf_probe_disarm(void);
display_perf_tap_fn display_perf_tap(void);

/* `perf` with any subcommand but fps. */
int display_perf_command(int argc, char **argv);

/* The fields `perf fps` and `perf window stop` share, from shown on. */
void display_perf_reply_window(td_reply_t *r, const display_perf_window_t *w);

/* Reply helpers the perf command files share. td_reply has no null, no number with a fraction and no list of objects,
 * so display_perf_reply_printf appends text the way td_reply does: nothing more once a write did not fit, and
 * td_reply_end then answers "reply too long". */
void display_perf_send(td_reply_t *r);
void display_perf_send_error(const char *message);
void display_perf_reply_printf(td_reply_t *r, const char *fmt, ...) __attribute__((format(printf, 2, 3)));
void display_perf_reply_null(td_reply_t *r, const char *key);
void display_perf_reply_tenths(td_reply_t *r, const char *key, int32_t tenths);
void display_perf_reply_task_name(td_reply_t *r, const char *name);

/* A plain decimal word from min to max. */
bool display_perf_parse_uint(const char *word, uint32_t min, uint32_t max, uint32_t *out);

/* Scratch that prefers PSRAM, so taking a measurement leaves internal RAM as it was. */
void *display_perf_alloc_scratch(size_t size);

/* display_perf_fb.c */
void display_perf_cmd_fb(int argc, char **argv);
void display_perf_cmd_invalidate(int argc, char **argv);
void display_perf_cmd_stall(int argc, char **argv);

/* display_perf_mem.c */
void display_perf_cmd_mem_boot(void);
void display_perf_cmd_mem_last(void);
void display_perf_cmd_mem_heaps(void);
void display_perf_cmd_memtest(int argc, char **argv);
void display_perf_cmd_membw(void);
uint32_t display_perf_boot_count(void);

/* The error text for a wrong `perf` command, and the hint `help` shows. Keep the two lists in step. */
#define DISPLAY_PERF_USAGE \
    "usage: perf fps <seconds 1-60> | info | window start|stop | tap <x> <y> | mem [boot|last|heaps] " \
    "| cpu <seconds 1-60> | fb prep|check | invalidate <x> <y> <w> <h> | stall <ms 1-10000> " \
    "| memtest <kb> | membw | temp"
#define DISPLAY_PERF_HINT \
    "fps <seconds> | info | window start|stop | tap <x> <y> | mem [boot|last|heaps] | cpu <seconds> " \
    "| fb prep|check | invalidate <x> <y> <w> <h> | stall <ms> | memtest <kb> | membw | temp"
#endif

typedef struct {
    uint32_t seconds;
    uint32_t frames;
    uint32_t elapsed_ms;
#if CONFIG_TD_PERF_TOOLS
    display_perf_window_t tools;
#endif
} display_perf_t;

esp_err_t panel_rgb_create(size_t num_fbs, esp_lcd_panel_handle_t *out);

/* Renders the active screen on the LVGL task into the one PSRAM snapshot buffer and returns a view of it. Only
 * the console task calls it, so the view stays valid until its next call. ESP_ERR_INVALID_STATE while a timed-out
 * request's job has not run yet, ESP_ERR_TIMEOUT when LVGL is too busy to lock or to finish in time. */
esp_err_t display_snapshot(td_shot_image_t *out);

#if CONFIG_TD_PAY_COPY_DEBT
#include "lvgl.h"

/* CONFIG_TD_PAY_COPY_DEBT, in display_copy_debt.c: after a refresh that redrew most of the screen, brings the two
 * lagging triple-partial frame buffers up to date while LVGL is idle. Call it with the adapter lock held. */
void display_copy_debt_attach(lv_display_t *disp);

/* The idle refreshes it has asked for since boot. */
uint32_t display_copy_debt_paid(void);
#endif

#if CONFIG_TD_LAZY_SNAPSHOT_BUF
/* With the lazy buffer, display_snapshot allocates the buffer and its caller hands it back here once done with the
 * view, after a failure too. A job that a timed-out request left behind keeps it until that job has run. */
void display_snapshot_release(void);
#endif

/* Invalidates the whole screen every LVGL cycle for `seconds` and counts the frames flushed to the panel.
 * Blocks the caller for that time. */
esp_err_t display_perf_run(uint32_t seconds, display_perf_t *out);
