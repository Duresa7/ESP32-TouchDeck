#pragma once

#include <stdint.h>
#include "lvgl.h"
#include "ui_pages.h"

/* Colors are RGB565, because that is what the panel stores. A design color goes in as 8-bit channels and the
 * macro drops the low bits, so the value that a screenshot reads back is the token itself. Write red and blue as
 * multiples of 8 and green as a multiple of 4, or use 255, so the design color is the one the panel shows. Every
 * fill is one flat token, because LVGL 9.6 cannot dither a gradient, and the panel would band it and tint it
 * green. */
#define UI_RGB565(r, g, b) ((uint16_t)((((r) >> 3) << 11) | (((g) >> 2) << 5) | ((b) >> 3)))

/* Every color, size, shadow and animation time of the deck's look. The look is only read here. */
typedef struct {
    struct {
        uint16_t bg;
        uint16_t tile;
        uint16_t tile_down;
        uint16_t text;
        uint16_t title;
        uint16_t caption;
        uint16_t dot_idle;
        uint16_t dot_active;
        uint16_t glow;
        uint16_t mod_locked;
        uint16_t lock;
    } color;
    int16_t tile_radius;
    int16_t tile_down_border;
    int16_t glow_width;
    int16_t glow_spread;
    uint8_t glow_opa;
    int16_t mod_outline_width;
    int16_t mod_outline_pad;
    /* The lock glyph on a locked modifier tile. It fills a box of w by h, `inset` from the tile's right edge and
     * centred vertically. A shackle of shackle_w by shackle_h, drawn with a `stroke` wide line, sits at the top of
     * the box over a body of w by body_h at the bottom. */
    struct {
        int16_t w;
        int16_t h;
        int16_t shackle_w;
        int16_t shackle_h;
        int16_t stroke;
        int16_t body_h;
        int16_t radius;
        int16_t inset;
    } lock;
    /* A media tile stacks an icon over a caption. icon_dy and caption_dy are the pixels from the tile's centre to
     * the centre of each label, and the caption is label_inset narrower than the tile on each side. */
    struct {
        int16_t icon_dy;
        int16_t caption_dy;
        int16_t label_inset;
    } media;
    uint16_t mod_poll_ms;
    ui_rect_t band;
    ui_rect_t grid;
    int16_t gap;
    int16_t title_x;
    int16_t title_y;
    int16_t dot_y;
    int16_t dot_pitch;
    int16_t dot_size;
    int16_t dot_active_extra;
    int16_t bounce_px;
    uint16_t anim_settle_ms;
    uint16_t anim_bounce_out_ms;
    uint16_t anim_bounce_back_ms;
    uint16_t anim_press_ms;
    uint16_t anim_dot_ms;
    const lv_font_t *font_tile;
    const lv_font_t *font_title;
    const lv_font_t *font_caption;
} ui_theme_t;

extern const ui_theme_t ui_theme;

typedef struct {
    lv_style_t tile;
    lv_style_t tile_down;
    lv_style_t tile_label;
    lv_style_t tile_caption;
    lv_style_t title;
    lv_style_t dot;
    lv_style_t dot_on;
    lv_style_t mod_one_shot;
    lv_style_t mod_locked;
    lv_style_t lock_shackle;
    lv_style_t lock_body;
} ui_styles_t;

lv_color_t ui_color(uint16_t rgb565);

/* Builds the shared styles once, and returns them on every later call. The caller holds the LVGL lock. */
const ui_styles_t *ui_styles(void);

/* The x of the centre of dot `index` when `count` dots sit in a row centred on the screen. */
int16_t ui_dot_x(uint8_t index, uint8_t count);
