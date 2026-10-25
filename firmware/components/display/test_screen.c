#include "test_screen.h"
#include <stdatomic.h>
#include "esp_log.h"
#include "lvgl.h"

const uint16_t td_test_bars_rgb565[TD_TEST_BAR_COUNT] = {
    0xFFFF, 0xFFE0, 0x07FF, 0x07E0, 0xF81F, 0xF800, 0x001F, 0x0000,
};

#define BAR_WIDTH 128
#define PAD_WIDTH 440
#define PAD_HEIGHT 100
#define PAD_TOP 40
#define PAD_RADIUS 24
#define BUTTON_RADIUS 24
#define BUTTON_COLOR 0x2D6CDF
#define BUTTON_COLOR_PRESSED 0x6FA0F5

_Static_assert(TD_TEST_BUTTON_Y > TD_TEST_BAR_PROBE_Y, "the button must stay clear of the bar pixels the recipe reads");

static const char *TAG = "ui";

static atomic_uint s_taps;
static lv_obj_t *s_taps_label;

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

uint32_t test_screen_taps(void)
{
    return atomic_load(&s_taps);
}

/* Runs on the LVGL task, which holds the adapter lock. */
static void on_button_click(lv_event_t *e)
{
    (void)e;
    const unsigned taps = atomic_fetch_add(&s_taps, 1) + 1;
    lv_label_set_text_fmt(s_taps_label, "Taps: %u", taps);
    ESP_LOGI(TAG, "click taps=%u", taps);
}

static void add_tap_button(lv_obj_t *screen)
{
    lv_obj_t *button = lv_button_create(screen);
    lv_obj_remove_style_all(button);
    lv_obj_set_pos(button, TD_TEST_BUTTON_X, TD_TEST_BUTTON_Y);
    lv_obj_set_size(button, TD_TEST_BUTTON_W, TD_TEST_BUTTON_H);
    lv_obj_set_style_radius(button, BUTTON_RADIUS, 0);
    lv_obj_set_style_bg_opa(button, LV_OPA_COVER, 0);
    lv_obj_set_style_bg_color(button, lv_color_hex(BUTTON_COLOR), 0);
    lv_obj_set_style_bg_color(button, lv_color_hex(BUTTON_COLOR_PRESSED), LV_STATE_PRESSED);

    s_taps_label = lv_label_create(button);
    lv_label_set_text(s_taps_label, "Taps: 0");
    lv_obj_set_style_text_font(s_taps_label, &lv_font_montserrat_48, 0);
    lv_obj_set_style_text_color(s_taps_label, lv_color_white(), 0);
    lv_obj_center(s_taps_label);

    lv_obj_add_event_cb(button, on_button_click, LV_EVENT_CLICKED, NULL);
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

    add_tap_button(screen);
}
