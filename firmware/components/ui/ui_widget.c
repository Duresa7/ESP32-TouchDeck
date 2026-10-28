#include "ui.h"
#include "ui_priv.h"
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "esp_heap_caps.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"
#include "ui_theme.h"

/* The labels and bar of one widget tile, at the same index as its entry in the table. */
typedef struct {
    lv_obj_t *value;
    lv_obj_t *caption;
    lv_obj_t *bar;
    const char *label;
    int32_t w;
} widget_objs_t;

/*
 * The table is written by the link task and the console, and read by the redraw timer, under `lock`. The object lists
 * belong to the LVGL task. `dimmed` is what the screen shows, and the console reads it and `dim_count` without a lock.
 * The deck boots dimmed, because no app has said hello yet.
 */
static struct {
    SemaphoreHandle_t lock;
    ui_widget_table_t *table;
    widget_objs_t widgets[UI_WIDGET_TILES_MAX];
    lv_obj_t *dim[TD_LAYOUT_MAX_TILES];
    uint16_t dim_total;
    lv_timer_t *timer;
    bool (*online)(void);
    atomic_bool dimmed;
    atomic_uint dim_count;
} s = { .dimmed = true };

static uint32_t now_ms(void)
{
    return (uint32_t)(esp_timer_get_time() / 1000);
}

static void table_lock(void)
{
    xSemaphoreTake(s.lock, portMAX_DELAY);
}

static void table_unlock(void)
{
    xSemaphoreGive(s.lock);
}

void ui_set_link_online(bool (*online)(void))
{
    s.online = online;
}

bool ui_link_online(void)
{
    bool (*online)(void) = s.online;
    return online != NULL && online();
}

static bool dims(const ui_tile_t *tile)
{
    return tile->action.type == TD_ACTION_APP || tile->action.type == TD_ACTION_WIDGET;
}

static void apply_dim(bool dimmed)
{
    for (uint16_t i = 0; i < s.dim_total; i++) {
        lv_obj_set_state(s.dim[i], LV_STATE_USER_2, dimmed);
    }
    atomic_store(&s.dimmed, dimmed);
    atomic_store(&s.dim_count, dimmed ? s.dim_total : 0);
}

/* A value that fits keeps its content width, so it sits where it always did. A wider one is cut to the tile with dots. */
static void draw_value(lv_obj_t *value, const char *text, int32_t tile_w)
{
    lv_point_t size;
    lv_text_get_size(&size, text, ui_theme.font_tile, 0, 0, LV_COORD_MAX, LV_TEXT_FLAG_NONE);
    const int32_t max_w = tile_w - 2 * ui_theme.media.label_inset;
    lv_obj_set_width(value, size.x > max_w ? max_w : LV_SIZE_CONTENT);
    lv_label_set_text(value, text);
}

static void draw_widget(const widget_objs_t *o, const ui_widget_state_t *st)
{
    draw_value(o->value, st->value[0] != '\0' ? st->value : "--", o->w);
    lv_label_set_text(o->caption, st->caption[0] != '\0' ? st->caption : o->label);
    lv_obj_set_style_text_opa(o->value, st->stale ? ui_theme.stale_opa : LV_OPA_COVER, 0);
    lv_obj_set_style_text_opa(o->caption, st->stale ? ui_theme.stale_opa : LV_OPA_COVER, 0);
    const int32_t bar_w = st->bar < 0.0f ? 0 : (int32_t)(st->bar * (float)o->w + 0.5f);
    lv_obj_set_width(o->bar, bar_w);
    lv_obj_set_hidden(o->bar, bar_w == 0);
}

/* A burst of sets marks entries dirty many times, and this draws each one once. The copies are taken under the lock
 * and drawn after it, so the link task never waits on LVGL. */
static void on_redraw(lv_timer_t *timer)
{
    (void)timer;
    const bool offline = !ui_link_online();
    if (offline != atomic_load(&s.dimmed)) {
        apply_dim(offline);
    }
    if (s.table == NULL) {
        return;
    }
    ui_widget_state_t widget;
    ui_dash_state_t dash;
    for (int i = 0; i < UI_WIDGET_TILES_MAX; i++) {
        bool dirty = false;
        table_lock();
        if (i < s.table->widget_count && s.table->widgets[i].dirty) {
            widget = s.table->widgets[i].state;
            s.table->widgets[i].dirty = false;
            dirty = true;
        }
        table_unlock();
        if (dirty && s.widgets[i].value != NULL) {
            draw_widget(&s.widgets[i], &widget);
        }
    }
    for (int i = 0; i < UI_DASH_PAGES_MAX; i++) {
        bool dirty = false;
        table_lock();
        if (i < s.table->dashboard_count && s.table->dashboards[i].dirty) {
            dash = s.table->dashboards[i].state;
            s.table->dashboards[i].dirty = false;
            dirty = true;
        }
        table_unlock();
        if (dirty) {
            ui_dashboard_draw(i, &dash);
        }
    }
}

void ui_widget_begin(void)
{
    if (s.lock == NULL) {
        s.lock = xSemaphoreCreateMutex();
        s.table = heap_caps_calloc(1, sizeof *s.table, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
        if (s.table == NULL) {
            s.table = calloc(1, sizeof *s.table);
        }
    }
    if (s.table != NULL) {
        table_lock();
        ui_widget_table_clear(s.table);
        table_unlock();
    }
    memset(s.widgets, 0, sizeof s.widgets);
    memset(s.dim, 0, sizeof s.dim);
    s.dim_total = 0;
    atomic_store(&s.dim_count, 0);
    ui_dashboard_begin();
    if (s.timer == NULL) {
        s.timer = lv_timer_create(on_redraw, ui_theme.widget.redraw_ms, NULL);
    }
}

void ui_widget_build(lv_obj_t *obj, const ui_tile_t *tile, ui_rect_t rect)
{
    const ui_styles_t *st = ui_styles();
    lv_obj_t *value = lv_label_create(obj);
    lv_obj_add_style(value, &st->tile_label, 0);
    lv_obj_set_style_text_align(value, LV_TEXT_ALIGN_CENTER, 0);
    lv_obj_set_height(value, lv_font_get_line_height(ui_theme.font_tile));
    lv_label_set_long_mode(value, LV_LABEL_LONG_MODE_DOTS);
    draw_value(value, "--", rect.w);
    lv_obj_align(value, LV_ALIGN_CENTER, 0, -ui_theme.widget.value_dy);

    /* One line, cut with dots, whatever text the app sends. LVGL's dots mode needs its own writable copy of the text. */
    lv_obj_t *caption = lv_label_create(obj);
    lv_obj_add_style(caption, &st->tile_caption, 0);
    lv_obj_set_size(caption, rect.w - 2 * ui_theme.media.label_inset, lv_font_get_line_height(ui_theme.font_caption));
    lv_label_set_long_mode(caption, LV_LABEL_LONG_MODE_DOTS);
    lv_label_set_text(caption, tile->label);
    lv_obj_align(caption, LV_ALIGN_CENTER, 0, ui_theme.widget.caption_dy);

    lv_obj_t *bar = lv_obj_create(obj);
    lv_obj_remove_style_all(bar);
    lv_obj_add_style(bar, &st->widget_bar, 0);
    lv_obj_set_size(bar, 0, ui_theme.widget.bar_h);
    lv_obj_set_pos(bar, 0, rect.h - ui_theme.widget.bar_bottom - ui_theme.widget.bar_h);
    lv_obj_set_clickable(bar, false);
    lv_obj_set_scrollable(bar, false);
    lv_obj_set_hidden(bar, true);

    if (s.table == NULL) {
        return;
    }
    table_lock();
    const bool added = ui_widget_table_add_tile(s.table, tile->id) == UI_WIDGET_OK;
    const int index = added ? ui_widget_index(s.table, tile->id) : -1;
    table_unlock();
    if (index >= 0) {
        s.widgets[index] = (widget_objs_t){
            .value = value, .caption = caption, .bar = bar, .label = tile->label, .w = rect.w,
        };
    }
}

void ui_widget_track(lv_obj_t *obj, const ui_tile_t *tile)
{
    if (!dims(tile) || s.dim_total >= TD_LAYOUT_MAX_TILES) {
        return;
    }
    lv_obj_add_style(obj, &ui_styles()->tile_dim, LV_STATE_USER_2);
    s.dim[s.dim_total++] = obj;
    const bool dimmed = atomic_load(&s.dimmed);
    lv_obj_set_state(obj, LV_STATE_USER_2, dimmed);
    atomic_store(&s.dim_count, dimmed ? s.dim_total : 0);
}

bool ui_widget_tap_blocked(const ui_tile_t *tile)
{
    return dims(tile) && atomic_load(&s.dimmed);
}

uint16_t ui_widget_dimmed(void)
{
    return (uint16_t)atomic_load(&s.dim_count);
}

bool ui_widget_add_page(const char *page)
{
    if (s.table == NULL) {
        return false;
    }
    table_lock();
    const bool added = ui_widget_table_add_page(s.table, page) == UI_WIDGET_OK;
    table_unlock();
    return added;
}

bool ui_widget_read(const char *tile, ui_widget_state_t *out)
{
    if (s.table == NULL) {
        return false;
    }
    table_lock();
    const ui_widget_state_t *st = ui_widget_get(s.table, tile);
    if (st != NULL) {
        *out = *st;
    }
    table_unlock();
    return st != NULL;
}

bool ui_dashboard_read(const char *page, ui_dash_state_t *out)
{
    if (s.table == NULL) {
        return false;
    }
    table_lock();
    const ui_dash_state_t *st = ui_dashboard_get(s.table, page);
    if (st != NULL) {
        *out = *st;
    }
    table_unlock();
    return st != NULL;
}

const char *ui_widget_update(const char *tile, const char *value, const char *caption, float bar, bool stale,
                             ui_widget_state_t *stored)
{
    if (s.table == NULL) {
        return "display not ready";
    }
    table_lock();
    const ui_widget_result_t result = ui_widget_set(s.table, tile, value, caption, bar, stale, now_ms());
    if (result == UI_WIDGET_OK && stored != NULL) {
        *stored = *ui_widget_get(s.table, tile);
    }
    table_unlock();
    return result == UI_WIDGET_OK ? NULL : ui_widget_result_text(result, false);
}

const char *ui_dashboard_update(const char *page, const ui_dash_row_t *rows, size_t count)
{
    if (s.table == NULL) {
        return "display not ready";
    }
    table_lock();
    const ui_widget_result_t result = ui_dashboard_set(s.table, page, rows, count, now_ms());
    table_unlock();
    return result == UI_WIDGET_OK ? NULL : ui_widget_result_text(result, true);
}
