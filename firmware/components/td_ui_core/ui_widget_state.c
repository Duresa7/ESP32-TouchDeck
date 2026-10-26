#include "ui_widget_state.h"
#include <math.h>
#include <string.h>

const char *ui_widget_result_text(ui_widget_result_t result, bool dashboard)
{
    switch (result) {
    case UI_WIDGET_OK:
        return "";
    case UI_WIDGET_UNKNOWN:
        return dashboard ? "unknown page" : "unknown tile";
    case UI_WIDGET_TOO_MANY:
        return "too many rows";
    default:
        return "table full";
    }
}

void ui_widget_table_clear(ui_widget_table_t *table)
{
    memset(table, 0, sizeof *table);
}

void ui_widget_copy_text(char *dst, size_t cap, const char *src)
{
    if (cap == 0) {
        return;
    }
    size_t len = src != NULL ? strlen(src) : 0;
    if (len >= cap) {
        len = cap - 1;
        /* Back up over continuation bytes, so the cut never splits a character. */
        while (len > 0 && ((unsigned char)src[len] & 0xc0) == 0x80) {
            len--;
        }
    }
    if (len > 0) {
        memcpy(dst, src, len);
    }
    dst[len] = '\0';
}

float ui_widget_clamp_bar(float bar)
{
    if (isnan(bar) || bar < 0.0f) {
        return -1.0f;
    }
    return bar > 1.0f ? 1.0f : bar;
}

int ui_widget_index(const ui_widget_table_t *table, const char *tile)
{
    for (int i = 0; tile != NULL && i < table->widget_count; i++) {
        if (strcmp(table->widgets[i].id, tile) == 0) {
            return i;
        }
    }
    return -1;
}

int ui_dashboard_index(const ui_widget_table_t *table, const char *page)
{
    for (int i = 0; page != NULL && i < table->dashboard_count; i++) {
        if (strcmp(table->dashboards[i].id, page) == 0) {
            return i;
        }
    }
    return -1;
}

static bool usable_id(const char *id)
{
    return id != NULL && id[0] != '\0' && strlen(id) <= UI_WIDGET_ID_MAX;
}

ui_widget_result_t ui_widget_table_add_tile(ui_widget_table_t *table, const char *id)
{
    if (!usable_id(id)) {
        return UI_WIDGET_UNKNOWN;
    }
    if (ui_widget_index(table, id) >= 0) {
        return UI_WIDGET_OK;
    }
    if (table->widget_count >= UI_WIDGET_TILES_MAX) {
        return UI_WIDGET_FULL;
    }
    memset(&table->widgets[table->widget_count], 0, sizeof table->widgets[0]);
    strcpy(table->widgets[table->widget_count].id, id);
    table->widgets[table->widget_count].state.bar = -1.0f;
    table->widget_count++;
    return UI_WIDGET_OK;
}

ui_widget_result_t ui_widget_table_add_page(ui_widget_table_t *table, const char *id)
{
    if (!usable_id(id)) {
        return UI_WIDGET_UNKNOWN;
    }
    if (ui_dashboard_index(table, id) >= 0) {
        return UI_WIDGET_OK;
    }
    if (table->dashboard_count >= UI_DASH_PAGES_MAX) {
        return UI_WIDGET_FULL;
    }
    memset(&table->dashboards[table->dashboard_count], 0, sizeof table->dashboards[0]);
    strcpy(table->dashboards[table->dashboard_count].id, id);
    table->dashboard_count++;
    return UI_WIDGET_OK;
}

ui_widget_result_t ui_widget_set(ui_widget_table_t *table, const char *tile, const char *value, const char *caption,
                                 float bar, bool stale, uint32_t now_ms)
{
    const int i = ui_widget_index(table, tile);
    if (i < 0) {
        return UI_WIDGET_UNKNOWN;
    }
    ui_widget_state_t *st = &table->widgets[i].state;
    ui_widget_copy_text(st->value, sizeof st->value, value);
    ui_widget_copy_text(st->caption, sizeof st->caption, caption);
    st->bar = ui_widget_clamp_bar(bar);
    st->stale = stale;
    st->updated_ms = now_ms;
    table->widgets[i].dirty = true;
    return UI_WIDGET_OK;
}

ui_widget_result_t ui_dashboard_set(ui_widget_table_t *table, const char *page, const ui_dash_row_t *rows,
                                    size_t count, uint32_t now_ms)
{
    const int i = ui_dashboard_index(table, page);
    if (i < 0) {
        return UI_WIDGET_UNKNOWN;
    }
    if (count > UI_DASH_ROWS_MAX) {
        return UI_WIDGET_TOO_MANY;
    }
    ui_dash_state_t *st = &table->dashboards[i].state;
    memset(st->rows, 0, sizeof st->rows);
    for (size_t r = 0; r < count; r++) {
        ui_widget_copy_text(st->rows[r].label, sizeof st->rows[r].label, rows[r].label);
        ui_widget_copy_text(st->rows[r].value, sizeof st->rows[r].value, rows[r].value);
        st->rows[r].bar = ui_widget_clamp_bar(rows[r].bar);
    }
    st->count = (uint8_t)count;
    st->updated_ms = now_ms;
    table->dashboards[i].dirty = true;
    return UI_WIDGET_OK;
}

const ui_widget_state_t *ui_widget_get(const ui_widget_table_t *table, const char *tile)
{
    const int i = ui_widget_index(table, tile);
    return i >= 0 ? &table->widgets[i].state : NULL;
}

const ui_dash_state_t *ui_dashboard_get(const ui_widget_table_t *table, const char *page)
{
    const int i = ui_dashboard_index(table, page);
    return i >= 0 ? &table->dashboards[i].state : NULL;
}
