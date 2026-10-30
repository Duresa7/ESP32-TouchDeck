#include "ui.h"
#include "ui_priv.h"
#include <stdlib.h>
#include "esp_heap_caps.h"
#include "esp_log.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"
#include "ui_graph.h"
#include "ui_theme.h"

static const char *TAG = "live";

/* The objects of one graph Tile, at the same index as its slot. `points` is what the line draws, so it lives as long
 * as the line. w and h are the plot area's size. */
typedef struct {
    lv_obj_t *area;
    lv_obj_t *line;
    lv_obj_t *label;
    lv_obj_t *marker;
    int16_t w;
    int16_t h;
    lv_point_precise_t points[UI_LIVE_SERIES_MAX];
} graph_objs_t;

/*
 * The table is written by the link task and read by the redraw timer, under `lock`. The slots and objects are
 * allocated when a Layout is activated, for the streams it declares and at most UI_LIVE_SLOTS_MAX. The objects belong
 * to the LVGL task.
 */
static struct {
    SemaphoreHandle_t lock;
    ui_live_table_t table;
    ui_live_slot_t *slots;
    graph_objs_t *objs;
    lv_timer_t *timer;
} s;

static uint32_t now_ms(void)
{
    return (uint32_t)(esp_timer_get_time() / 1000);
}

static void *alloc(size_t n, size_t size)
{
    void *p = heap_caps_calloc(n, size, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
    return p != NULL ? p : calloc(n, size);
}

static void draw(graph_objs_t *o, const ui_live_slot_t *slot)
{
    double samples[UI_LIVE_SERIES_MAX];
    ui_graph_point_t points[UI_LIVE_SERIES_MAX];
    const uint8_t count = ui_graph_points(samples, ui_live_samples(slot, samples), UI_LIVE_SERIES_MAX, o->w, o->h,
                                          points);
    for (uint8_t i = 0; i < count; i++) {
        o->points[i] = (lv_point_precise_t){ points[i].x, points[i].y };
    }
    lv_line_set_points(o->line, o->points, count);
    lv_obj_set_hidden(o->line, count < 2);
    const lv_opa_t opa = slot->stale ? ui_theme.stale_opa : LV_OPA_COVER;
    lv_obj_set_style_opa(o->area, opa, 0);
    lv_obj_set_style_opa(o->label, opa, 0);
    lv_obj_set_hidden(o->marker, slot->set && !slot->stale);
}

/* Expiry and offline marks are taken under the lock, and the dirty slots are copied out and drawn after it, so the
 * link task never waits on LVGL. */
static void on_redraw(lv_timer_t *timer)
{
    (void)timer;
    if (s.lock == NULL || s.objs == NULL) {
        return;
    }
    const bool offline = !ui_link_online();
    xSemaphoreTake(s.lock, portMAX_DELAY);
    const uint16_t count = s.table.count;
    if (offline) {
        ui_live_offline(&s.table);
    }
    ui_live_expire(&s.table, now_ms());
    xSemaphoreGive(s.lock);
    for (uint16_t i = 0; i < count; i++) {
        ui_live_slot_t slot;
        bool dirty = false;
        xSemaphoreTake(s.lock, portMAX_DELAY);
        if (s.table.slots[i].dirty) {
            slot = s.table.slots[i];
            s.table.slots[i].dirty = false;
            dirty = true;
        }
        xSemaphoreGive(s.lock);
        if (dirty && s.objs[i].line != NULL) {
            draw(&s.objs[i], &slot);
        }
    }
}

void ui_live_begin(const td_layout_t *layout)
{
    if (s.lock == NULL) {
        s.lock = xSemaphoreCreateMutex();
    }
    size_t want = ui_live_streams(layout->pages, layout->page_count);
    want = want > UI_LIVE_SLOTS_MAX ? UI_LIVE_SLOTS_MAX : want;
    ui_live_slot_t *slots = want > 0 ? alloc(want, sizeof *slots) : NULL;
    graph_objs_t *objs = want > 0 ? alloc(want, sizeof *objs) : NULL;
    if (slots == NULL || objs == NULL) {
        free(slots);
        free(objs);
        slots = NULL;
        objs = NULL;
    }
    xSemaphoreTake(s.lock, portMAX_DELAY);
    ui_live_slot_t *old_slots = s.slots;
    graph_objs_t *old_objs = s.objs;
    s.slots = slots;
    s.objs = objs;
    ui_live_bind(&s.table, slots, (uint16_t)(slots != NULL ? want : 0), layout->builtin ? "builtin" : layout->sha256,
                 layout->pages, layout->page_count);
    xSemaphoreGive(s.lock);
    free(old_slots);
    free(old_objs);
    if (s.table.dropped > 0 || (want > 0 && slots == NULL)) {
        ESP_LOGW(TAG, "%u live streams have no slot", (unsigned)(s.table.dropped + (slots == NULL ? want : 0)));
    }
    if (s.timer == NULL) {
        s.timer = lv_timer_create(on_redraw, ui_theme.widget.redraw_ms, NULL);
    }
}

void ui_live_build(lv_obj_t *obj, const ui_tile_t *tile, ui_rect_t rect)
{
    const ui_styles_t *st = ui_styles();
    const int32_t pad = ui_theme.graph.pad;
    const int32_t top = ui_theme.graph.top;
    const int32_t line_h = lv_font_get_line_height(ui_theme.font_label);

    lv_obj_t *label = lv_label_create(obj);
    lv_obj_add_style(label, &st->tile_label, 0);
    lv_obj_set_style_text_align(label, LV_TEXT_ALIGN_LEFT, 0);
    lv_obj_set_size(label, rect.w - 2 * pad, line_h);
    lv_label_set_long_mode(label, LV_LABEL_LONG_MODE_DOTS);
    lv_label_set_text(label, tile->label);
    lv_obj_set_pos(label, pad, (top - line_h) / 2);

    lv_obj_t *marker = lv_label_create(obj);
    lv_obj_add_style(marker, &st->tile_label, 0);
    lv_label_set_text_static(marker, "--");
    lv_obj_align(marker, LV_ALIGN_TOP_RIGHT, -pad, (top - line_h) / 2);

    const int16_t w = (int16_t)LV_MAX(rect.w - 2 * pad, 0);
    const int16_t h = (int16_t)LV_MAX(rect.h - top - pad, 0);
    lv_obj_t *area = lv_obj_create(obj);
    lv_obj_remove_style_all(area);
    lv_obj_add_style(area, &st->graph_area, 0);
    lv_obj_set_pos(area, pad, top);
    lv_obj_set_size(area, w, h);
    lv_obj_set_clickable(area, false);
    lv_obj_set_scrollable(area, false);

    lv_obj_t *line = lv_line_create(area);
    lv_obj_remove_style_all(line);
    lv_obj_add_style(line, &st->graph_line, 0);
    lv_obj_set_pos(line, 0, 0);
    lv_obj_set_size(line, w, h);
    lv_obj_set_clickable(line, false);
    lv_obj_set_hidden(line, true);

    if (s.lock == NULL || s.objs == NULL || tile->stream_count == 0) {
        return;
    }
    xSemaphoreTake(s.lock, portMAX_DELAY);
    const ui_live_slot_t *slot = ui_live_find(&s.table, tile, tile->streams[0].id);
    const int index = slot != NULL ? (int)(slot - s.table.slots) : -1;
    xSemaphoreGive(s.lock);
    if (index >= 0) {
        s.objs[index] = (graph_objs_t){ .area = area, .line = line, .label = label, .marker = marker, .w = w, .h = h };
    }
}

void ui_live_select(void)
{
    if (s.lock == NULL) {
        return;
    }
    xSemaphoreTake(s.lock, portMAX_DELAY);
    ui_live_session(&s.table, now_ms());
    xSemaphoreGive(s.lock);
}

ui_live_result_t ui_live_update(const ui_live_set_t *set, uint32_t *retry_ms)
{
    if (s.lock == NULL) {
        return UI_LIVE_LAYOUT;
    }
    xSemaphoreTake(s.lock, portMAX_DELAY);
    const ui_live_result_t result = ui_live_set(&s.table, set, now_ms(), retry_ms);
    xSemaphoreGive(s.lock);
    return result;
}
