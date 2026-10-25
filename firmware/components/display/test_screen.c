#include "test_screen.h"
#include "lvgl.h"

const uint16_t td_test_bars_rgb565[TD_TEST_BAR_COUNT] = {
    0xFFFF, 0xFFE0, 0x07FF, 0x07E0, 0xF81F, 0xF800, 0x001F, 0x0000,
};

#define BAR_WIDTH 128
#define PAD_WIDTH 440
#define PAD_HEIGHT 100
#define PAD_TOP 40
#define PAD_RADIUS 24

/* Keeps the low bits zero, so the conversion back to RGB565 returns exactly the table value. */
static lv_color_t color_from_rgb565(uint16_t v)
{
    return lv_color_make((uint8_t)(((v >> 11) & 0x1F) << 3), (uint8_t)(((v >> 5) & 0x3F) << 2),
                         (uint8_t)((v & 0x1F) << 3));
}

static lv_obj_t *solid_box(lv_obj_t *parent, int32_t w, int32_t h, lv_color_t color)
{
    lv_obj_t *box = lv_obj_create(parent);
    lv_obj_remove_style_all(box);
    lv_obj_set_size(box, w, h);
    lv_obj_set_style_bg_color(box, color, 0);
    lv_obj_set_style_bg_opa(box, LV_OPA_COVER, 0);
    return box;
}

void test_screen_build(void)
{
    lv_obj_t *screen = lv_screen_active();
    lv_obj_set_scrollable(screen, false);
    lv_obj_set_style_bg_color(screen, lv_color_black(), 0);
    lv_obj_set_style_bg_opa(screen, LV_OPA_COVER, 0);

    for (int i = 0; i < TD_TEST_BAR_COUNT; i++) {
        lv_obj_t *bar = solid_box(screen, BAR_WIDTH, LV_PCT(100), color_from_rgb565(td_test_bars_rgb565[i]));
        lv_obj_set_pos(bar, i * BAR_WIDTH, 0);
    }

    lv_obj_t *pad = solid_box(screen, PAD_WIDTH, PAD_HEIGHT, lv_color_black());
    lv_obj_set_style_radius(pad, PAD_RADIUS, 0);
    lv_obj_align(pad, LV_ALIGN_TOP_MID, 0, PAD_TOP);

    lv_obj_t *label = lv_label_create(pad);
    lv_label_set_text(label, "TouchDeck");
    lv_obj_set_style_text_font(label, &lv_font_montserrat_48, 0);
    lv_obj_set_style_text_color(label, lv_color_white(), 0);
    lv_obj_center(label);
}
