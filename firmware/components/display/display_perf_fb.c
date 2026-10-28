#include "display.h"
#include "display_priv.h"
#include <stdlib.h>
#include <string.h>
#include "board_pins.h"
#include "esp_heap_caps.h"
#include "esp_lcd_panel_rgb.h"
#include "esp_lv_adapter.h"
#include "esp_lv_adapter_display.h"
#include "esp_rom_crc.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "lvgl.h"
#include "lvgl_private.h"

/*
 * `perf fb prep` and `perf fb check` compare the panel's frame buffers, what the panel scans out, with LVGL's own
 * render of the screen. They are separate, so a check never redraws over the evidence it is about to read.
 *
 * prep invalidates the whole screen three times and waits for each redraw. Triple buffering hands each finished
 * frame to the panel and draws the next one into the buffer that has waited longest, so three full redraws of a
 * still screen leave the same frame in all three buffers.
 *
 * check redraws nothing. It takes a snapshot, then copies one panel buffer at a time to PSRAM scratch with the
 * adapter lock held, and compares the copy with the snapshot after the lock is released. Three copies and the
 * snapshot would need 4.9 MB of PSRAM, more than the deck has free, so the buffers go one by one. Each copy first
 * checks that no frame has flushed since the snapshot and that LVGL holds nothing it has invalidated and not drawn,
 * so every buffer is compared with the screen it was drawn from. If anything moved it answers "screen changed".
 * `submits` counts the frames the adapter handed to the panel since the last prep, so the bench can tell that the
 * partial-update frames it asked for between them went through every buffer.
 *
 * In portrait LVGL draws a 600 by 1024 screen and the adapter turns it a quarter turn into the 1024 by 600 panel
 * buffers. The comparison finds each panel pixel's snapshot pixel with the adapter's mapping, written out here one
 * pixel at a time, so the check does not go through td_rotate, which is what it tests.
 */

#define FB_MAX 3
#define PREP_REDRAWS 3
#define REDRAW_TIMEOUT_MS 2000
#define POLL_MS 2
#define SETTLE_TIMEOUT_MS 2000
#define SETTLE_POLL_MS 20
#define LOCK_TIMEOUT_MS 3000
#define FB_ALIGN 64
#define FB_REPLY_MAX 1024
#define STALL_MAX_MS 10000
#define LVGL_TASK_NAME "lvgl"

typedef struct {
    uint32_t crc32;
    bool exact;
    uint32_t max_channel_diff;
    uint32_t diff_pixels;
} fb_result_t;

static esp_timer_handle_t s_stall_timer;
static TaskHandle_t s_stalled;
/* The adapter's hand-overs to the panel when the last prep ended, so check can say how many frames followed it. */
static bool s_prepped;
static uint32_t s_prep_submits;

/* The LVGL task holds the lock for one refresh at most, so waiting for it is bounded. */
static void lock(void)
{
    (void)esp_lv_adapter_lock(-1);
}

/* Waits until the frame count passes `after`. */
static bool wait_frame_after(uint32_t after, uint32_t timeout_ms)
{
    const int64_t until = esp_timer_get_time() + (int64_t)timeout_ms * 1000;
    while ((int32_t)(display_frame_count() - after) <= 0) {
        if (esp_timer_get_time() > until) {
            return false;
        }
        vTaskDelay(pdMS_TO_TICKS(POLL_MS));
    }
    return true;
}

/* Invalidated with the lock held, so the next refresh, which cannot start before the lock is free, draws all of it.
 * That refresh is the first to flush after the count read here. */
static esp_err_t full_redraw(void)
{
    lock();
    const uint32_t before = display_frame_count();
    lv_obj_invalidate(lv_screen_active());
    esp_lv_adapter_unlock();
    return wait_frame_after(before, REDRAW_TIMEOUT_MS) ? ESP_OK : ESP_ERR_TIMEOUT;
}

static void cmd_prep(void)
{
    const int64_t t0 = esp_timer_get_time();
    for (int i = 0; i < PREP_REDRAWS; i++) {
        if (full_redraw() != ESP_OK) {
            display_perf_send_error("redraw timed out");
            return;
        }
    }
    s_prep_submits = display_perf_submits();
    s_prepped = true;
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "perf", true);
    td_reply_str(&r, "op", "fb");
    td_reply_str(&r, "phase", "prep");
    td_reply_int(&r, "redraws", PREP_REDRAWS);
    td_reply_int(&r, "elapsed_ms", (esp_timer_get_time() - t0) / 1000);
    display_perf_send(&r);
}

/* The snapshot pixel that panel pixel (row, col) shows, for the adapter's rotations: ROTATE_90 puts logical (x, y) at
 * panel row x, column W - 1 - y, and ROTATE_270 at panel row H - 1 - x, column y. */
static uint16_t reference_px(const td_shot_image_t *img, int rotation, int row, int col)
{
    int x = col;
    int y = row;
    if (rotation == ESP_LV_ADAPTER_ROTATE_90) {
        x = row;
        y = TD_LCD_H_RES - 1 - col;
    } else if (rotation == ESP_LV_ADAPTER_ROTATE_270) {
        x = TD_LCD_V_RES - 1 - row;
        y = col;
    }
    const uint8_t *line = (const uint8_t *)img->px + (size_t)y * img->stride;
    return ((const uint16_t *)line)[x];
}

static uint32_t channel_diff(uint16_t a, uint16_t b)
{
    const int dr = abs((int)(a >> 11) - (int)(b >> 11));
    const int dg = abs((int)((a >> 5) & 0x3f) - (int)((b >> 5) & 0x3f));
    const int db = abs((int)(a & 0x1f) - (int)(b & 0x1f));
    const int d = dr > dg ? dr : dg;
    return (uint32_t)(d > db ? d : db);
}

static void compare(const uint16_t *fb, const td_shot_image_t *img, int rotation, fb_result_t *out)
{
    *out = (fb_result_t){ .crc32 = esp_rom_crc32_le(0, (const uint8_t *)fb, TD_LCD_FRAME_BYTES) };
    for (int row = 0; row < TD_LCD_V_RES; row++) {
        const uint16_t *line = fb + (size_t)row * TD_LCD_H_RES;
        for (int col = 0; col < TD_LCD_H_RES; col++) {
            const uint16_t want = reference_px(img, rotation, row, col);
            if (line[col] != want) {
                out->diff_pixels++;
                const uint32_t d = channel_diff(line[col], want);
                out->max_channel_diff = d > out->max_channel_diff ? d : out->max_channel_diff;
            }
        }
    }
    out->exact = out->diff_pixels == 0;
}

static bool snapshot_fits(const td_shot_image_t *img, int rotation)
{
    const bool quarter = rotation == ESP_LV_ADAPTER_ROTATE_90 || rotation == ESP_LV_ADAPTER_ROTATE_270;
    if (!quarter && rotation != ESP_LV_ADAPTER_ROTATE_0) {
        return false;
    }
    return img->width == (quarter ? TD_LCD_V_RES : TD_LCD_H_RES) &&
           img->height == (quarter ? TD_LCD_H_RES : TD_LCD_V_RES) && img->stride >= img->width * 2U;
}

/* Waits until LVGL has drawn everything it was asked to, and returns the frame count of that moment. */
static bool settle(uint32_t *frames)
{
    const int64_t until = esp_timer_get_time() + SETTLE_TIMEOUT_MS * 1000LL;
    while (true) {
        lock();
        const bool pending = display_perf_redraw_pending();
        *frames = display_frame_count();
        esp_lv_adapter_unlock();
        if (!pending) {
            return true;
        }
        if (esp_timer_get_time() > until) {
            return false;
        }
        vTaskDelay(pdMS_TO_TICKS(SETTLE_POLL_MS));
    }
}

static void cmd_check(void)
{
    const display_perf_config_t *c = display_perf_config();
    const size_t n = c->num_fbs < FB_MAX ? c->num_fbs : FB_MAX;
    void *fbs[FB_MAX] = { 0 };
    if (display_perf_panel() == NULL || n == 0 ||
        esp_lcd_rgb_panel_get_frame_buffer(display_perf_panel(), n, &fbs[0], &fbs[1], &fbs[2]) != ESP_OK) {
        display_perf_send_error("no frame buffers");
        return;
    }

    uint32_t frames = 0;
    if (!settle(&frames)) {
        display_perf_send_error("screen busy");
        return;
    }
    td_shot_image_t img;
    esp_err_t err = display_snapshot(&img);
    if (err != ESP_OK) {
        display_perf_send_error(err == ESP_ERR_INVALID_STATE ? "busy" :
                                err == ESP_ERR_TIMEOUT ? "display busy" : "snapshot failed");
        return;
    }
    if (!snapshot_fits(&img, c->rotation)) {
        display_perf_send_error("snapshot size");
        return;
    }
    uint16_t *copy = heap_caps_aligned_alloc(FB_ALIGN, TD_LCD_FRAME_BYTES, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
    char *buf = display_perf_alloc_scratch(FB_REPLY_MAX);
    if (copy == NULL || buf == NULL) {
        heap_caps_free(copy);
        free(buf);
        display_perf_send_error("no memory");
        return;
    }

    fb_result_t results[FB_MAX];
    uint32_t lock_us_max = 0;
    bool changed = false;
    for (size_t i = 0; i < n && !changed; i++) {
        lock();
        const int64_t t0 = esp_timer_get_time();
        changed = display_frame_count() != frames || display_perf_redraw_pending();
        if (!changed) {
            memcpy(copy, fbs[i], TD_LCD_FRAME_BYTES);
        }
        const uint32_t lock_us = (uint32_t)(esp_timer_get_time() - t0);
        esp_lv_adapter_unlock();
        lock_us_max = lock_us > lock_us_max ? lock_us : lock_us_max;
        if (!changed) {
            compare(copy, &img, c->rotation, &results[i]);
        }
    }
    const uint32_t snapshot_crc = esp_rom_crc32_le(0, img.px, (uint32_t)img.stride * img.height);
    const uint32_t submits = display_perf_submits();
    heap_caps_free(copy);
    if (changed) {
        free(buf);
        display_perf_send_error("screen changed");
        return;
    }

    td_reply_t r;
    bool all_exact = true;
    td_reply_begin(&r, buf, FB_REPLY_MAX, "perf", true);
    td_reply_str(&r, "op", "fb");
    td_reply_str(&r, "phase", "check");
    td_reply_int(&r, "rotation", c->rotation);
    if (s_prepped) {
        td_reply_int(&r, "submits", submits - s_prep_submits);
    } else {
        display_perf_reply_null(&r, "submits");
    }
    td_reply_int(&r, "snapshot_crc", snapshot_crc);
    td_reply_int(&r, "copy_lock_us_max", lock_us_max);
    display_perf_reply_printf(&r, ",\"buffers\":[");
    for (size_t i = 0; i < n; i++) {
        all_exact = all_exact && results[i].exact;
        display_perf_reply_printf(&r,
                                  "%s{\"index\":%u,\"crc\":%lu,\"exact\":%s,\"max_diff\":%lu,\"diff_pixels\":%lu}",
                                  i == 0 ? "" : ",", (unsigned)i, (unsigned long)results[i].crc32,
                                  results[i].exact ? "true" : "false", (unsigned long)results[i].max_channel_diff,
                                  (unsigned long)results[i].diff_pixels);
    }
    display_perf_reply_printf(&r, "]");
    td_reply_bool(&r, "all_exact", all_exact);
    display_perf_send(&r);
    free(buf);
}

void display_perf_cmd_fb(int argc, char **argv)
{
    if (argc == 3 && strcmp(argv[2], "prep") == 0) {
        cmd_prep();
    } else if (argc == 3 && strcmp(argv[2], "check") == 0) {
        cmd_check();
    } else {
        display_perf_send_error("usage: perf fb prep|check");
    }
}

/* Marks one area for redraw and returns at once. The bench counts the frames that follow with `perf window`. */
void display_perf_cmd_invalidate(int argc, char **argv)
{
    const uint32_t w_max = display_width();
    const uint32_t h_max = display_height();
    uint32_t x = 0;
    uint32_t y = 0;
    uint32_t w = 0;
    uint32_t h = 0;
    if (argc != 6 || !display_perf_parse_uint(argv[2], 0, w_max - 1, &x) ||
        !display_perf_parse_uint(argv[3], 0, h_max - 1, &y) || !display_perf_parse_uint(argv[4], 1, w_max - x, &w) ||
        !display_perf_parse_uint(argv[5], 1, h_max - y, &h)) {
        display_perf_send_error("usage: perf invalidate <x> <y> <w> <h>, inside the screen");
        return;
    }
    const lv_area_t area = { .x1 = (int32_t)x, .y1 = (int32_t)y, .x2 = (int32_t)(x + w - 1),
                             .y2 = (int32_t)(y + h - 1) };
    if (esp_lv_adapter_lock(LOCK_TIMEOUT_MS) != ESP_OK) {
        display_perf_send_error("display busy");
        return;
    }
    lv_inv_area(display_perf_display(), &area);
    esp_lv_adapter_unlock();

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "perf", true);
    td_reply_str(&r, "op", "invalidate");
    td_reply_int(&r, "x", x);
    td_reply_int(&r, "y", y);
    td_reply_int(&r, "w", w);
    td_reply_int(&r, "h", h);
    display_perf_send(&r);
}

static void stall_end(void *arg)
{
    (void)arg;
    vTaskResume(s_stalled);
    s_stalled = NULL;
}

/* Suspends the LVGL task for a while and replies at once, so a `screenshot` sent next queues its job and runs into
 * its timeout. The task is suspended with the adapter lock held by this one, so it is not inside a refresh then. The
 * panel keeps scanning out the last frame. */
void display_perf_cmd_stall(int argc, char **argv)
{
    uint32_t ms = 0;
    if (argc != 3 || !display_perf_parse_uint(argv[2], 1, STALL_MAX_MS, &ms)) {
        display_perf_send_error("usage: perf stall <ms 1-10000>");
        return;
    }
    if (s_stalled != NULL) {
        display_perf_send_error("already stalled");
        return;
    }
    TaskHandle_t lvgl = xTaskGetHandle(LVGL_TASK_NAME);
    if (lvgl == NULL) {
        display_perf_send_error("no lvgl task");
        return;
    }
    if (s_stall_timer == NULL) {
        const esp_timer_create_args_t args = { .callback = stall_end, .name = "td_perf_stall" };
        if (esp_timer_create(&args, &s_stall_timer) != ESP_OK) {
            display_perf_send_error("no timer");
            return;
        }
    }
    if (esp_lv_adapter_lock(LOCK_TIMEOUT_MS) != ESP_OK) {
        display_perf_send_error("display busy");
        return;
    }
    s_stalled = lvgl;
    vTaskSuspend(lvgl);
    esp_lv_adapter_unlock();
    ESP_ERROR_CHECK(esp_timer_start_once(s_stall_timer, (uint64_t)ms * 1000));

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "perf", true);
    td_reply_str(&r, "op", "stall");
    td_reply_int(&r, "ms", ms);
    display_perf_send(&r);
}
