#include "ui.h"
#include "ui_priv.h"
#include "ui_theme.h"
#include <string.h>
#include "esp_log.h"
#include "esp_lv_adapter.h"
#include "lvgl.h"

static const char *TAG = "ui";

#define LOCK_TIMEOUT_MS 3000
#define CANCEL_W 280
#define CANCEL_H 96

/* The code lives here only while the screen shows it. Every field is touched under the adapter lock. */
static struct {
    bool shown;
    char code[8];
    void (*on_cancel)(void);
    lv_obj_t *overlay;
} s;

static void on_cancel_click(lv_event_t *e)
{
    (void)e;
    ESP_LOGI(TAG, "pairing cancel");
    if (s.on_cancel) {
        s.on_cancel();
    }
}

static void on_overlay_delete(lv_event_t *e)
{
    (void)e;
    s.overlay = NULL;
}

static lv_obj_t *add_label(lv_obj_t *parent, const char *text, const lv_font_t *font, uint16_t color, int32_t y)
{
    lv_obj_t *label = lv_label_create(parent);
    lv_label_set_text(label, text);
    lv_obj_set_style_text_font(label, font, 0);
    lv_obj_set_style_text_color(label, ui_color(color), 0);
    lv_obj_align(label, LV_ALIGN_TOP_MID, 0, y);
    return label;
}

/* Covers the deck as the last child of the active screen, so the screenshot shows it and it takes every touch. */
void ui_pairing_redraw(void)
{
    if (s.overlay) {
        lv_obj_delete(s.overlay);
    }
    if (!s.shown) {
        return;
    }
    const ui_geometry_t *g = ui_geo();
    lv_obj_t *overlay = lv_obj_create(lv_screen_active());
    lv_obj_remove_style_all(overlay);
    lv_obj_set_scrollable(overlay, false);
    lv_obj_set_pos(overlay, 0, 0);
    lv_obj_set_size(overlay, g->screen.w, g->screen.h);
    lv_obj_set_style_bg_color(overlay, ui_color(ui_theme.color.bg), 0);
    lv_obj_set_style_bg_opa(overlay, LV_OPA_COVER, 0);
    lv_obj_add_event_cb(overlay, on_overlay_delete, LV_EVENT_DELETE, NULL);
    lv_obj_move_foreground(overlay);
    s.overlay = overlay;

    const int32_t top = g->screen.h / 2 - 190;
    add_label(overlay, "Pair with the TouchDeck app", ui_theme.font_title, ui_theme.color.title, top);
    add_label(overlay, "Type this code in the app", ui_theme.font_caption, ui_theme.color.caption, top + 56);
    char spaced[8];
    memcpy(spaced, s.code, 3);
    spaced[3] = ' ';
    memcpy(spaced + 4, s.code + 3, 3);
    spaced[7] = '\0';
    lv_obj_t *code = add_label(overlay, spaced, ui_theme.font_tile, ui_theme.color.text, top + 130);
    lv_obj_set_style_text_letter_space(code, 18, 0);

    lv_obj_t *cancel = lv_button_create(overlay);
    lv_obj_remove_style_all(cancel);
    lv_obj_set_size(cancel, CANCEL_W, CANCEL_H);
    lv_obj_align(cancel, LV_ALIGN_TOP_MID, 0, top + 250);
    lv_obj_set_style_bg_color(cancel, ui_color(ui_theme.color.tile), 0);
    lv_obj_set_style_bg_color(cancel, ui_color(ui_theme.color.tile_down), LV_STATE_PRESSED);
    lv_obj_set_style_bg_opa(cancel, LV_OPA_COVER, 0);
    lv_obj_set_style_radius(cancel, ui_theme.tile_radius, 0);
    lv_obj_add_event_cb(cancel, on_cancel_click, LV_EVENT_CLICKED, NULL);
    lv_obj_t *label = lv_label_create(cancel);
    lv_label_set_text_static(label, "Cancel");
    lv_obj_set_style_text_font(label, ui_theme.font_caption, 0);
    lv_obj_set_style_text_color(label, ui_color(ui_theme.color.text), 0);
    lv_obj_center(label);
}

static void set(const char *code, void (*on_cancel)(void))
{
    if (!ui_ready() || esp_lv_adapter_lock(LOCK_TIMEOUT_MS) != ESP_OK) {
        ESP_LOGW(TAG, "pairing screen not drawn: display not ready");
        return;
    }
    s.shown = code != NULL;
    s.on_cancel = on_cancel;
    memset(s.code, 0, sizeof s.code);
    if (code) {
        memcpy(s.code, code, 6);
    }
    ui_pairing_redraw();
    esp_lv_adapter_unlock();
}

void ui_pairing_show(const char code[6], void (*on_cancel)(void))
{
    set(code, on_cancel);
}

void ui_pairing_hide(void)
{
    set(NULL, NULL);
}
