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

/* The screen colors of app/ui/src/styles/tokens.css in RGB565, one macro per token. */
#define UI_SCREEN_BG UI_RGB565(8, 16, 24)
#define UI_SCREEN_TILE UI_RGB565(24, 32, 48)
#define UI_SCREEN_TILE_RAISED UI_RGB565(32, 44, 64)
#define UI_SCREEN_TRACK UI_RGB565(48, 56, 72)
#define UI_SCREEN_PAGER_IDLE UI_RGB565(96, 100, 112)
#define UI_SCREEN_OUTLINE UI_RGB565(40, 44, 48)
#define UI_SCREEN_INK UI_RGB565(240, 240, 248)
#define UI_SCREEN_INK_2 UI_RGB565(144, 152, 176)
#define UI_SCREEN_ACCENT UI_RGB565(88, 168, 255)
#define UI_SCREEN_LIVE UI_RGB565(64, 216, 144)
#define UI_SCREEN_SIGNAL UI_RGB565(255, 176, 32)
#define UI_SCREEN_DANGER UI_RGB565(255, 88, 96)
#define UI_TILE_COLOR_VIOLET UI_RGB565(72, 60, 192)
#define UI_TILE_COLOR_GREEN UI_RGB565(16, 112, 80)
#define UI_TILE_COLOR_BLUE UI_RGB565(32, 92, 184)
#define UI_TILE_COLOR_RED UI_RGB565(176, 40, 48)
#define UI_TILE_COLOR_ORANGE UI_RGB565(160, 88, 16)
#define UI_TILE_COLOR_SLATE UI_RGB565(96, 100, 112)
#define UI_TILE_ACCENT_BACKLIGHT UI_RGB565(88, 168, 255)
#define UI_TILE_ACCENT_VIOLET UI_RGB565(160, 140, 255)
#define UI_TILE_ACCENT_CORAL UI_RGB565(255, 136, 104)
#define UI_TILE_ACCENT_MINT UI_RGB565(96, 212, 192)

/* The Archivo instances in fonts/, generated as docs/firmware.md, "Fonts", says. */
LV_FONT_DECLARE(ui_font_medium_22)
LV_FONT_DECLARE(ui_font_medium_26)
LV_FONT_DECLARE(ui_font_medium_44)
LV_FONT_DECLARE(ui_font_semibold_32)
LV_FONT_DECLARE(ui_font_semibold_40)
LV_FONT_DECLARE(ui_font_nameplate_26)

/* Every color, size and animation time of the deck's look. The look is only read here. Where things sit on the
 * screen depends on the orientation, so that lives in ui_geo(). */
typedef struct {
    struct {
        uint16_t bg;
        uint16_t tile;
        uint16_t tile_raised;
        uint16_t track;
        uint16_t pager_idle;
        uint16_t ink;
        uint16_t ink_2;
        uint16_t accent;
        uint16_t signal;
    } color;
    int16_t tile_radius;
    /* The inset ring of a toggled-on tile, a locked modifier, in ink, and of a one-shot modifier, in the accent. */
    int16_t ring_width;
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
    /* An icon sits `gap` above its label, and a label stays label_inset clear of the tile's sides. */
    struct {
        int16_t gap;
        int16_t label_inset;
    } media;
    uint16_t mod_poll_ms;
    /* App and widget tiles fade to dim_opa while the app is offline. A stale widget value draws at stale_opa. */
    uint8_t dim_opa;
    uint8_t stale_opa;
    /* A widget tile: the value in the big font `value_dy` above the centre, the caption in the label font
     * `caption_dy` below it, and a bar_h tall bar on a track, `pad` in from the tile's sides and bar_bottom above its
     * bottom edge, filled to the bar fraction. The app's sets reach the screen on a redraw_ms timer. */
    struct {
        int16_t value_dy;
        int16_t caption_dy;
        int16_t pad;
        int16_t bar_h;
        int16_t bar_bottom;
        uint16_t redraw_ms;
    } widget;
    /* A dashboard page: UI_DASH_ROWS_MAX rows of row_gap apart filling the grid area, each a tile-colored strip with
     * the label at `pad`, the value at the geometry's dash_value_x, and a bar_h tall bar on a track from dash_bar_x
     * to `pad` before the row's end. */
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
    /* A tile's own text label, in Basic Latin. A label with other letters starts at font_label. */
    const lv_font_t *font_legend;
    /* A label under an icon, a widget caption and a dashboard label. */
    const lv_font_t *font_label;
    /* The last step down for a tile label too wide for its tile in the legend and label fonts. */
    const lv_font_t *font_sub;
    /* A dashboard value. */
    const lv_font_t *font_value;
    /* A widget's one big value. */
    const lv_font_t *font_big;
    /* The Page title, the Recovery caption and the boot name. */
    const lv_font_t *font_title;
    /* The `lv:` icons, which are LVGL symbols that only the built-in Montserrat carries. */
    const lv_font_t *font_icon;
} ui_theme_t;

extern const ui_theme_t ui_theme;

typedef struct {
    lv_style_t tile;
    lv_style_t tile_down;
    lv_style_t tile_legend;
    lv_style_t tile_label;
    lv_style_t tile_icon;
    lv_style_t widget_value;
    lv_style_t dash_value;
    lv_style_t title;
    lv_style_t dot;
    lv_style_t dot_on;
    lv_style_t mod_one_shot;
    lv_style_t mod_locked;
    lv_style_t lock_shackle;
    lv_style_t lock_body;
    lv_style_t tile_dim;
    lv_style_t bar_track;
    lv_style_t bar_fill;
    lv_style_t dash_row;
} ui_styles_t;

lv_color_t ui_color(uint16_t rgb565);

/* Builds the shared styles once, and returns them on every later call. The caller holds the LVGL lock. */
const ui_styles_t *ui_styles(void);

/* The band, the grid, the title, the pager and the pager distances for the screen display_start set up. */
const ui_geometry_t *ui_geo(void);

/* Picks the full-screen geometry, with no title or pager strips, or the default one, and works it out at once. A
 * layout load calls it under the layout lock before it builds anything, so readers under that lock see one geometry. */
void ui_geo_set_full_screen(bool full_screen);
