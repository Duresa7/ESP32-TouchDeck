#include "ui.h"
#include "ui_priv.h"
#include <string.h>
#include "esp32s3/rom/tjpgd.h"
#include "esp_heap_caps.h"
#include "esp_log.h"
#include "esp_lv_adapter.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "lvgl.h"
#include "power.h"
#include "td_idle.h"
#include "ui_theme.h"

#define TICK_MS 250
#define LOCK_TIMEOUT_MS 3000
#define JPEG_WORK_BYTES 4096
#define CLOCK_SCALE (3 * LV_SCALE_NONE)

static const char *TAG = "idle";

static struct {
    lv_obj_t *face;
    lv_obj_t *picture;
    lv_obj_t *clock;
    uint16_t *pixels;
    lv_image_dsc_t dsc[2];
    const lv_image_dsc_t *shown;
    uint8_t dsc_next;
    int64_t picture_us;
    char text[6];
    lv_timer_t *timer;
} s;

static portMUX_TYPE s_lock = portMUX_INITIALIZER_UNLOCKED;
static td_clock_t s_clock;
static uint32_t s_interval_s = TD_IDLE_INTERVAL_DEFAULT_S;

static void on_face_deleted(lv_event_t *e)
{
    (void)e;
    s.face = NULL;
    s.picture = NULL;
    s.clock = NULL;
}

static void drop_picture(void)
{
    if (s.picture != NULL) {
        lv_image_set_src(s.picture, NULL);
    }
    heap_caps_free(s.pixels);
    s.pixels = NULL;
    s.shown = NULL;
}

static void build_face(void)
{
    const ui_geometry_t *g = ui_geo();
    s.face = lv_obj_create(lv_screen_active());
    lv_obj_remove_style_all(s.face);
    lv_obj_set_pos(s.face, 0, 0);
    lv_obj_set_size(s.face, g->screen.w, g->screen.h);
    lv_obj_set_style_bg_color(s.face, lv_color_black(), 0);
    lv_obj_set_style_bg_opa(s.face, LV_OPA_COVER, 0);
    lv_obj_set_clickable(s.face, false);
    lv_obj_set_scrollable(s.face, false);
    lv_obj_add_event_cb(s.face, on_face_deleted, LV_EVENT_DELETE, NULL);

    s.picture = lv_image_create(s.face);
    lv_obj_set_hidden(s.picture, true);
    if (s.shown != NULL) {
        lv_image_set_src(s.picture, s.shown);
    }
    lv_obj_center(s.picture);

    s.clock = lv_label_create(s.face);
    lv_obj_set_style_text_font(s.clock, ui_theme.font_value, 0);
    lv_obj_set_style_text_color(s.clock, ui_color(ui_theme.color.ink), 0);
    lv_obj_set_style_transform_scale(s.clock, CLOCK_SCALE, 0);
    lv_obj_set_style_transform_pivot_x(s.clock, lv_pct(50), 0);
    lv_obj_set_style_transform_pivot_y(s.clock, lv_pct(50), 0);
    s.text[0] = '\0';
    lv_label_set_text(s.clock, "--:--");
    lv_obj_center(s.clock);
    ESP_LOGI(TAG, "face up");
}

static void refresh_locked(int64_t now_us)
{
    portENTER_CRITICAL(&s_lock);
    const td_clock_t clock = s_clock;
    const uint32_t interval_s = s_interval_s;
    portEXIT_CRITICAL(&s_lock);

    const td_idle_face_t face = td_idle_face(ui_link_online(), s.pixels != NULL, s.picture_us, interval_s, now_us);
    lv_obj_set_hidden(s.picture, face != TD_FACE_PICTURE);
    lv_obj_set_hidden(s.clock, face != TD_FACE_CLOCK);

    char text[sizeof s.text];
    const bool synced = td_clock_text(&clock, now_us, text);
    if (strcmp(text, s.text) != 0) {
        memcpy(s.text, text, sizeof text);
        lv_label_set_text(s.clock, text);
        lv_obj_set_style_text_opa(s.clock, synced ? LV_OPA_COVER : ui_theme.dim_opa, 0);
        lv_obj_center(s.clock);
    }
}

static void tick(lv_timer_t *timer)
{
    (void)timer;
    if (!power_idle_face(NULL)) {
        if (s.face != NULL) {
            lv_obj_delete(s.face);
            ESP_LOGI(TAG, "face down");
        }
        drop_picture();
        return;
    }
    if (s.face == NULL) {
        build_face();
    } else if (lv_obj_get_index(s.face) != (int32_t)lv_obj_get_child_count(lv_screen_active()) - 1) {
        lv_obj_move_to_index(s.face, -1);
    }
    refresh_locked(esp_timer_get_time());
}

void ui_idle_start(void)
{
    if (s.timer == NULL) {
        s.timer = lv_timer_create(tick, TICK_MS, NULL);
    }
}

void ui_idle_set_clock(uint64_t unix_s, int32_t offset_min)
{
    const int64_t now_us = esp_timer_get_time();
    portENTER_CRITICAL(&s_lock);
    td_clock_set(&s_clock, unix_s, offset_min, now_us);
    portEXIT_CRITICAL(&s_lock);
}

void ui_idle_set_interval(uint32_t interval_s)
{
    portENTER_CRITICAL(&s_lock);
    s_interval_s = interval_s;
    portEXIT_CRITICAL(&s_lock);
}

typedef struct {
    const uint8_t *jpeg;
    size_t len;
    size_t at;
    uint16_t *pixels;
    uint32_t width;
} decode_t;

static UINT jpeg_in(JDEC *jd, BYTE *buf, UINT want)
{
    decode_t *d = jd->device;
    const size_t left = d->len - d->at;
    const UINT n = want < left ? want : (UINT)left;
    if (buf != NULL) {
        memcpy(buf, d->jpeg + d->at, n);
    }
    d->at += n;
    return n;
}

/* TJpgDec hands over RGB888 blocks; the screen is RGB565. */
static UINT jpeg_out(JDEC *jd, void *bitmap, JRECT *rect)
{
    decode_t *d = jd->device;
    const BYTE *rgb = bitmap;
    for (UINT y = rect->top; y <= rect->bottom; y++) {
        uint16_t *row = d->pixels + (size_t)y * d->width + rect->left;
        for (UINT x = rect->left; x <= rect->right; x++, rgb += 3) {
            *row++ = (uint16_t)(((rgb[0] & 0xf8) << 8) | ((rgb[1] & 0xfc) << 3) | (rgb[2] >> 3));
        }
    }
    return 1;
}

ui_picture_t ui_idle_picture(const uint8_t *jpeg, size_t len)
{
    const int64_t started_us = esp_timer_get_time();
    void *work = heap_caps_malloc(JPEG_WORK_BYTES, MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT);
    if (work == NULL) {
        return UI_PICTURE_MEMORY;
    }
    decode_t d = { .jpeg = jpeg, .len = len };
    JDEC jd;
    JRESULT res = jd_prepare(&jd, jpeg_in, work, JPEG_WORK_BYTES, &d);
    const ui_geometry_t *g = ui_geo();
    if (res != JDR_OK || jd.width == 0 || jd.height == 0) {
        heap_caps_free(work);
        ESP_LOGW(TAG, "picture refused: jpeg %d", (int)res);
        return res == JDR_MEM1 || res == JDR_MEM2 ? UI_PICTURE_MEMORY : UI_PICTURE_FORMAT;
    }
    if (jd.width > (UINT)g->screen.w || jd.height > (UINT)g->screen.h) {
        heap_caps_free(work);
        ESP_LOGW(TAG, "picture refused: %ux%u is larger than the screen", (unsigned)jd.width, (unsigned)jd.height);
        return UI_PICTURE_SIZE;
    }
    if (!power_idle_face(NULL)) {
        heap_caps_free(work);
        return UI_PICTURE_IDLE;
    }
    const size_t bytes = (size_t)jd.width * jd.height * sizeof(uint16_t);
    d.pixels = heap_caps_malloc(bytes, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
    if (d.pixels == NULL) {
        heap_caps_free(work);
        return UI_PICTURE_MEMORY;
    }
    d.width = jd.width;
    res = jd_decomp(&jd, jpeg_out, 0);
    heap_caps_free(work);
    if (res != JDR_OK) {
        heap_caps_free(d.pixels);
        ESP_LOGW(TAG, "picture refused: jpeg decode %d", (int)res);
        return UI_PICTURE_FORMAT;
    }
    const uint32_t decode_ms = (uint32_t)((esp_timer_get_time() - started_us) / 1000);

    if (esp_lv_adapter_lock(LOCK_TIMEOUT_MS) != ESP_OK) {
        heap_caps_free(d.pixels);
        return UI_PICTURE_IDLE;
    }
    if (!power_idle_face(NULL)) {
        esp_lv_adapter_unlock();
        heap_caps_free(d.pixels);
        return UI_PICTURE_IDLE;
    }
    if (s.face == NULL) {
        build_face();
    }
    lv_image_dsc_t *dsc = &s.dsc[s.dsc_next];
    s.dsc_next ^= 1;
    *dsc = (lv_image_dsc_t){
        .header = { .magic = LV_IMAGE_HEADER_MAGIC, .cf = LV_COLOR_FORMAT_RGB565, .w = jd.width, .h = jd.height,
                    .stride = jd.width * sizeof(uint16_t) },
        .data_size = bytes,
        .data = (const uint8_t *)d.pixels,
    };
    uint16_t *old = s.pixels;
    s.shown = dsc;
    lv_image_set_src(s.picture, dsc);
    lv_obj_center(s.picture);
    s.pixels = d.pixels;
    s.picture_us = esp_timer_get_time();
    refresh_locked(s.picture_us);
    esp_lv_adapter_unlock();
    heap_caps_free(old);
    ESP_LOGI(TAG, "picture %ux%u from %u bytes, decoded in %u ms", (unsigned)jd.width, (unsigned)jd.height,
             (unsigned)len, (unsigned)decode_ms);
    return UI_PICTURE_SHOWN;
}
