#pragma once

#include <stdint.h>
#include "lvgl.h"
#include "ui_geometry.h"
#include "ui_pages.h"

/* Colors are RGB565, because that is what the panel stores. A design color goes in as 8-bit channels and the
 * macro drops the low bits, so the value that a screenshot reads back is the token itself. Write red and blue as
 * multiples of 8 and green as a multiple of 4, or use 255, so the design color is the one the panel shows. Every
 * fill is one flat token, because LVGL 9.6 cannot dither a gradient, and the panel would band it and tint it
 * green. */
#define UI_RGB565(r, g, b) ((uint16_t)((((r) >> 3) << 11) | (((g) >> 2) << 5) | ((b) >> 3)))

/* Every color, size, shadow and animation time of the deck's look. The look is only read here. Where things sit on
 * the screen depends on the orientation, so that lives in ui_geo(). */
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
    struct {
        int16_t gap;
        int16_t label_inset;
    } media;
    uint16_t mod_poll_ms;
    /* App and widget tiles fade to dim_opa while the app is offline. A stale widget value draws at stale_opa. */
    uint8_t dim_opa;
    uint8_t stale_opa;
    /* A widget tile: the value in the tile font `value_dy` above the centre, the caption in the caption font
     * `caption_dy` below it, and a bar_h tall bar in the glow color, `bar_bottom` above the tile's bottom edge, as
     * wide as the tile times the bar fraction. The app's sets reach the screen on a redraw_ms timer. */
    struct {
        int16_t value_dy;
        int16_t caption_dy;
        int16_t bar_h;
        int16_t bar_bottom;
        int16_t bar_radius;
        uint16_t redraw_ms;
    } widget;
    /* A dashboard page: UI_DASH_ROWS_MAX rows of row_gap apart filling the grid area, each a tile-colored strip with
     * the label at `pad`, the value at the geometry's dash_value_x, and a bar_h tall bar from dash_bar_x to `pad`
     * before the row's end. */
    struct {
        int16_t row_gap;
        int16_t radius;
        int16_t pad;
        int16_t bar_h;
    } dash;
    uint16_t anim_settle_ms;
    uint16_t anim_bounce_out_ms;
    uint16_t anim_bounce_back_ms;
    uint16_t anim_press_ms;
    uint16_t anim_dot_ms;
    const lv_font_t *font_tile;
    const lv_font_t *font_title;
    const lv_font_t *font_caption;
    /* The last step down for a tile label too wide for its tile in the tile and caption fonts. */
    const lv_font_t *font_small;
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
    lv_style_t tile_dim;
    lv_style_t widget_bar;
    lv_style_t dash_row;
} ui_styles_t;

lv_color_t ui_color(uint16_t rgb565);

/* Builds the shared styles once, and returns them on every later call. The caller holds the LVGL lock. */
const ui_styles_t *ui_styles(void);

/* The band, the grid, the title, the dots and the pager distances for the screen display_start set up. */
const ui_geometry_t *ui_geo(void);

/* Picks the full-screen geometry, with no title or dot strips, or the default one, and works it out at once. A layout
 * load calls it under the layout lock before it builds anything, so readers under that lock see one geometry. */
void ui_geo_set_full_screen(bool full_screen);

/* The x of the centre of dot `index` when `count` dots sit in a row centred on the screen. */
int16_t ui_dot_x(uint8_t index, uint8_t count);
