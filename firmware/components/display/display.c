#include "display.h"
#include "display_priv.h"
#include <stdatomic.h>
#include "board.h"
#include "board_pins.h"
#include "esp_check.h"
#include "esp_heap_caps.h"
#include "esp_lcd_panel_ops.h"
#include "esp_log.h"
#include "esp_lv_adapter.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"
#include "lvgl.h"

#define LVGL_TASK_STACK (12 * 1024)
/* Espressif's LCD FAQ says to run the LVGL task on the core that created the panel, so the bounce buffer
 * interrupt and the renderer do not fight over PSRAM. app_main runs on core 0. */
#define LVGL_TASK_CORE 0
#define LOCK_TIMEOUT_MS 3000
#define SNAPSHOT_TIMEOUT_MS 5000
#define FIRST_FRAME_TIMEOUT_MS 1500
#define SNAPSHOT_ALIGN 64
#define PERF_TICK_MS 1

static const char *TAG = "display";

static struct {
    lv_display_t *lv;
    lv_draw_buf_t shot;
    atomic_uint frames;
    SemaphoreHandle_t first_frame;
    SemaphoreHandle_t snap_done;
    atomic_bool snap_pending;
    bool snap_ok;
    atomic_bool ready;
    bool portrait;
} s;

/* Runs on the LVGL task after the adapter's flush callback returned. The last flush of a refresh
 * hands the whole back buffer to the panel, so each such event is one frame. */
static void on_flush_finish(lv_event_t *e)
{
    (void)e;
    if (!lv_display_flush_is_last(s.lv)) {
        return;
    }
    if (atomic_fetch_add(&s.frames, 1) == 0) {
        ESP_LOGI(TAG, "first frame");
        xSemaphoreGive(s.first_frame);
    }
}

/* The one buffer screenshots render into, the size of the rotated screen. It sits in PSRAM, and lv_draw_buf_init
 * needs lv_init to have run. */
static esp_err_t alloc_snapshot_buffer(void)
{
    void *data = heap_caps_aligned_alloc(SNAPSHOT_ALIGN, TD_LCD_FRAME_BYTES, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
    ESP_RETURN_ON_FALSE(data != NULL, ESP_ERR_NO_MEM, TAG, "snapshot buffer");
    lv_result_t res = lv_draw_buf_init(&s.shot, display_width(), display_height(), LV_COLOR_FORMAT_RGB565,
                                       LV_STRIDE_AUTO, data, TD_LCD_FRAME_BYTES);
    ESP_RETURN_ON_FALSE(res == LV_RESULT_OK, ESP_FAIL, TAG, "snapshot buffer init");
    return ESP_OK;
}

bool display_ready(void)
{
    return atomic_load(&s.ready);
}

uint32_t display_frame_count(void)
{
    return atomic_load(&s.frames);
}

bool display_portrait(void)
{
    return s.portrait;
}

uint16_t display_width(void)
{
    return s.portrait ? TD_LCD_V_RES : TD_LCD_H_RES;
}

uint16_t display_height(void)
{
    return s.portrait ? TD_LCD_H_RES : TD_LCD_V_RES;
}

esp_err_t display_start(bool portrait, display_build_fn build)
{
    s.portrait = portrait;
    s.first_frame = xSemaphoreCreateBinary();
    s.snap_done = xSemaphoreCreateBinary();
    ESP_RETURN_ON_FALSE(s.first_frame != NULL && s.snap_done != NULL, ESP_ERR_NO_MEM, TAG, "semaphores");

    /* The adapter rotates in software as it copies each flushed area into the panel's frame buffer, because the
     * ESP32-S3 has no PPA. LVGL draws the rotated screen, so a snapshot sees it the way the user does. */
    const esp_lv_adapter_rotation_t rotation = portrait ? ESP_LV_ADAPTER_ROTATE_90 : ESP_LV_ADAPTER_ROTATE_0;
    const esp_lv_adapter_tear_avoid_mode_t tear_mode = ESP_LV_ADAPTER_TEAR_AVOID_MODE_DEFAULT_RGB;
    const size_t fbs = esp_lv_adapter_get_required_frame_buffer_count(tear_mode, rotation);
#if CONFIG_TD_PERF_TOOLS
    display_perf_note_mode(tear_mode, rotation);
#endif

    esp_lcd_panel_handle_t panel = NULL;
    ESP_RETURN_ON_ERROR(panel_rgb_create(fbs, &panel), TAG, "rgb panel");
    ESP_RETURN_ON_ERROR(esp_lcd_panel_init(panel), TAG, "panel init");
#if CONFIG_TD_PERF_TOOLS
    display_perf_note_handle(panel);
    display_perf_boot_mark("panel");
#endif

    /* Without the expander the backlight stays off, but the panel and screenshots still work. */
    const esp_err_t board_err = board_init();
    if (board_err != ESP_OK) {
        ESP_LOGE(TAG, "board init failed, the backlight may stay off: %s", esp_err_to_name(board_err));
    }

    esp_lv_adapter_config_t adapter = ESP_LV_ADAPTER_DEFAULT_CONFIG();
    adapter.task_stack_size = LVGL_TASK_STACK;
    adapter.stack_in_psram = true;
    adapter.task_core_id = LVGL_TASK_CORE;
    ESP_RETURN_ON_ERROR(esp_lv_adapter_init(&adapter), TAG, "adapter init");
#if CONFIG_TD_PERF_TOOLS
    display_perf_boot_mark("adapter");
#endif
    ESP_RETURN_ON_ERROR(alloc_snapshot_buffer(), TAG, "snapshot buffer");

    esp_lv_adapter_display_config_t config =
        ESP_LV_ADAPTER_DISPLAY_RGB_DEFAULT_CONFIG(panel, NULL, TD_LCD_H_RES, TD_LCD_V_RES, rotation);
    config.profile.use_psram = true;
    s.lv = esp_lv_adapter_register_display(&config);
    ESP_RETURN_ON_FALSE(s.lv != NULL, ESP_FAIL, TAG, "register display");
    ESP_LOGI(TAG, "%s %ux%u", portrait ? "portrait" : "landscape", display_width(), display_height());
#if CONFIG_TD_PERF_TOOLS
    display_perf_boot_mark("register_display");
#endif

    ESP_RETURN_ON_ERROR(esp_lv_adapter_lock(-1), TAG, "adapter lock");
    lv_display_add_event_cb(s.lv, on_flush_finish, LV_EVENT_FLUSH_FINISH, NULL);
#if CONFIG_TD_PERF_TOOLS
    display_perf_attach(s.lv);
#endif
    if (build != NULL) {
        build();
    }
    esp_lv_adapter_unlock();
#if CONFIG_TD_PERF_TOOLS
    display_perf_boot_mark("ui_build");
#endif

    ESP_RETURN_ON_ERROR(esp_lv_adapter_start(), TAG, "adapter start");
    atomic_store(&s.ready, true);

    esp_err_t result = board_err;
    if (xSemaphoreTake(s.first_frame, pdMS_TO_TICKS(FIRST_FRAME_TIMEOUT_MS)) != pdTRUE) {
        ESP_LOGE(TAG, "no frame reached the panel within %d ms", FIRST_FRAME_TIMEOUT_MS);
        result = ESP_ERR_TIMEOUT;
    }
#if CONFIG_TD_PERF_TOOLS
    display_perf_boot_mark("first_frame");
#endif
    return result;
}

/* Runs on the LVGL task with the adapter lock held. Rendering a screen takes about 3.5 KB of stack, which
 * the 6 KB console task cannot spare, and the LVGL task has 12 KB. The token goes out before the flag clears,
 * so a request that follows a timed-out one cannot take the old token for its own job. */
static void snapshot_job(void *arg)
{
    (void)arg;
    s.snap_ok = lv_snapshot_take_to_draw_buf(lv_screen_active(), LV_COLOR_FORMAT_RGB565, &s.shot) == LV_RESULT_OK;
    xSemaphoreGive(s.snap_done);
    atomic_store(&s.snap_pending, false);
}

esp_err_t display_snapshot(td_shot_image_t *out)
{
    if (atomic_exchange(&s.snap_pending, true)) {
        return ESP_ERR_INVALID_STATE;
    }
    xSemaphoreTake(s.snap_done, 0);

    esp_err_t err = esp_lv_adapter_lock(LOCK_TIMEOUT_MS);
    if (err == ESP_OK) {
        err = lv_async_call(snapshot_job, NULL) == LV_RESULT_OK ? ESP_OK : ESP_FAIL;
        esp_lv_adapter_unlock();
    }
    if (err != ESP_OK) {
        atomic_store(&s.snap_pending, false);
        return err;
    }
    ESP_RETURN_ON_FALSE(xSemaphoreTake(s.snap_done, pdMS_TO_TICKS(SNAPSHOT_TIMEOUT_MS)) == pdTRUE, ESP_ERR_TIMEOUT, TAG,
                        "snapshot timed out");
    ESP_RETURN_ON_FALSE(s.snap_ok, ESP_FAIL, TAG, "snapshot");

    const lv_image_header_t *h = &s.shot.header;
    *out = (td_shot_image_t){ .px = s.shot.data, .width = (uint16_t)h->w, .height = (uint16_t)h->h, .stride = h->stride };
    return ESP_OK;
}

static void invalidate_screen(lv_timer_t *timer)
{
    (void)timer;
    lv_obj_invalidate(lv_screen_active());
}

esp_err_t display_perf_run(uint32_t seconds, display_perf_t *out)
{
    ESP_RETURN_ON_ERROR(esp_lv_adapter_lock(LOCK_TIMEOUT_MS), TAG, "adapter lock");
    lv_timer_t *timer = lv_timer_create(invalidate_screen, PERF_TICK_MS, NULL);
    esp_lv_adapter_unlock();
    ESP_RETURN_ON_FALSE(timer != NULL, ESP_ERR_NO_MEM, TAG, "perf timer");

#if CONFIG_TD_PERF_TOOLS
    display_perf_mark_t mark;
    display_perf_window_begin(DISPLAY_PERF_SLOT_FPS, &mark);
#endif
    const unsigned frames0 = atomic_load(&s.frames);
    const int64_t t0 = esp_timer_get_time();
    vTaskDelay(pdMS_TO_TICKS(seconds * 1000));
    const unsigned frames1 = atomic_load(&s.frames);
    const int64_t t1 = esp_timer_get_time();
#if CONFIG_TD_PERF_TOOLS
    display_perf_window_t tools;
    display_perf_window_end(DISPLAY_PERF_SLOT_FPS, &mark, &tools);
#endif

    /* The timer must not outlive the run, so wait for the lock however long the LVGL task holds it. */
    esp_err_t err = esp_lv_adapter_lock(-1);
    if (err == ESP_OK) {
        lv_timer_delete(timer);
        esp_lv_adapter_unlock();
    }
    *out = (display_perf_t){ .seconds = seconds, .frames = frames1 - frames0,
                             .elapsed_ms = (uint32_t)((t1 - t0) / 1000) };
#if CONFIG_TD_PERF_TOOLS
    out->tools = tools;
#endif
    return err;
}
