#include "wifi_glyph.h"
#include "display.h"
#include "esp_lv_adapter.h"
#include "lvgl.h"
#include "td_net.h"

#define GLYPH_UP 0x56c8a1
#define GLYPH_DOWN 0x596474
#define GLYPH_POLL_MS 500

static lv_obj_t *s_glyph;
static int s_up = -1;

/* The glyph is a label on the top layer, so a color change redraws only its own box and the deck's tiles stay as
 * they are. */
static void update(lv_timer_t *timer)
{
    (void)timer;
    const bool up = td_net_status() == TD_NET_CONNECTED;
    if (s_up != up) {
        lv_obj_set_style_text_color(s_glyph, lv_color_hex(up ? GLYPH_UP : GLYPH_DOWN), 0);
        s_up = up;
    }
}

void wifi_glyph_start(void)
{
    if (!display_ready() || esp_lv_adapter_lock(3000) != ESP_OK) return;
    s_glyph = lv_label_create(lv_layer_top());
    lv_label_set_text_static(s_glyph, LV_SYMBOL_WIFI);
    lv_obj_set_style_text_font(s_glyph, &lv_font_montserrat_28, 0);
    lv_obj_align(s_glyph, LV_ALIGN_TOP_RIGHT, -24, 16);
    lv_obj_set_clickable(s_glyph, false);
    update(NULL);
    lv_timer_create(update, GLYPH_POLL_MS, NULL);
    esp_lv_adapter_unlock();
}
