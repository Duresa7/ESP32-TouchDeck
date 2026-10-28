#include "ui_priv.h"
#include <string.h>
#include "ui_theme.h"

/* One row's objects. bar_max is the bar's width at a fraction of 1. */
typedef struct {
    lv_obj_t *row;
    lv_obj_t *label;
    lv_obj_t *value;
    lv_obj_t *bar;
    int32_t bar_max;
} row_objs_t;

/* The rows of each dashboard page, at the same index as the page's entry in the table. They belong to the LVGL task. */
static row_objs_t s_rows[UI_DASH_PAGES_MAX][UI_DASH_ROWS_MAX];
static uint8_t s_pages;

void ui_dashboard_begin(void)
{
    memset(s_rows, 0, sizeof s_rows);
    s_pages = 0;
}

/* Text wraps in its column for as many whole lines as a row_h tall row holds, and the last of them ends in dots when
 * there is more, so a long label or value never runs out of its row. Text that fits sits where it always did. LVGL's
 * dots mode needs its own writable copy of the text, which ui_dashboard_draw gives it. */
static lv_obj_t *row_label(lv_obj_t *row, int32_t x, int32_t w, int32_t row_h)
{
    const int32_t line_h = lv_font_get_line_height(ui_theme.font_caption);
    lv_obj_t *label = lv_label_create(row);
    lv_obj_add_style(label, &ui_styles()->tile_caption, 0);
    lv_obj_set_style_text_align(label, LV_TEXT_ALIGN_LEFT, 0);
    lv_obj_set_width(label, w);
    lv_obj_set_style_max_height(label, row_h / line_h * line_h, 0);
    lv_label_set_long_mode(label, LV_LABEL_LONG_MODE_DOTS);
    lv_label_set_text_static(label, "");
    lv_obj_align(label, LV_ALIGN_LEFT_MID, x, 0);
    return label;
}

/* The rows fill the grid area. They start hidden and show as the app sends them, so an empty page shows only its
 * title, or nothing at all on a full-screen layout. */
void ui_dashboard_build(lv_obj_t *page_obj, const ui_page_t *page)
{
    if (!ui_widget_add_page(page->id) || s_pages >= UI_DASH_PAGES_MAX) {
        return;
    }
    const ui_theme_t *t = &ui_theme;
    const ui_geometry_t *g = ui_geo();
    const ui_styles_t *st = ui_styles();
    const int32_t gap = t->dash.row_gap;
    const int32_t row_h = (g->grid.h - (UI_DASH_ROWS_MAX - 1) * gap) / UI_DASH_ROWS_MAX;
    const int32_t row_w = g->grid.w;
    row_objs_t *rows = s_rows[s_pages++];
    for (int i = 0; i < UI_DASH_ROWS_MAX; i++) {
        lv_obj_t *row = lv_obj_create(page_obj);
        lv_obj_remove_style_all(row);
        lv_obj_add_style(row, &st->dash_row, 0);
        lv_obj_set_pos(row, g->grid.x, g->grid.y - g->band.y + i * (row_h + gap));
        lv_obj_set_size(row, row_w, row_h);
        lv_obj_set_scrollable(row, false);
        lv_obj_set_clickable(row, false);
        lv_obj_set_hidden(row, true);

        rows[i].row = row;
        rows[i].label = row_label(row, t->dash.pad, g->dash_value_x - 2 * t->dash.pad, row_h);
        rows[i].value = row_label(row, g->dash_value_x, g->dash_bar_x - g->dash_value_x - t->dash.pad, row_h);
        lv_obj_set_style_text_color(rows[i].value, ui_color(t->color.text), 0);

        lv_obj_t *bar = lv_obj_create(row);
        lv_obj_remove_style_all(bar);
        lv_obj_add_style(bar, &st->widget_bar, 0);
        lv_obj_set_size(bar, 0, t->dash.bar_h);
        lv_obj_align(bar, LV_ALIGN_LEFT_MID, g->dash_bar_x, 0);
        lv_obj_set_clickable(bar, false);
        lv_obj_set_hidden(bar, true);
        rows[i].bar = bar;
        rows[i].bar_max = row_w - g->dash_bar_x - t->dash.pad;
    }
}

void ui_dashboard_draw(int index, const ui_dash_state_t *state)
{
    if (index < 0 || index >= s_pages) {
        return;
    }
    for (int i = 0; i < UI_DASH_ROWS_MAX; i++) {
        const row_objs_t *o = &s_rows[index][i];
        if (o->row == NULL) {
            continue;
        }
        const bool shown = i < state->count;
        lv_obj_set_hidden(o->row, !shown);
        if (!shown) {
            continue;
        }
        const ui_dash_row_t *r = &state->rows[i];
#if CONFIG_TD_UI_SKIP_NOOP_SETS
        ui_set_text_if_changed(o->label, r->label);
        ui_set_text_if_changed(o->value, r->value);
#else
        lv_label_set_text(o->label, r->label);
        lv_label_set_text(o->value, r->value);
#endif
        const int32_t bar_w = r->bar < 0.0f ? 0 : (int32_t)(r->bar * (float)o->bar_max + 0.5f);
        lv_obj_set_width(o->bar, bar_w);
        lv_obj_set_hidden(o->bar, bar_w == 0);
    }
}
