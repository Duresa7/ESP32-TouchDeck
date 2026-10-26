#include "ui_theme.h"
#include "board_pins.h"

const ui_theme_t ui_theme = {
    .color = {
        .bg = UI_RGB565(8, 8, 16),
        .tile = UI_RGB565(24, 32, 48),
        .tile_down = UI_RGB565(40, 64, 104),
        .text = UI_RGB565(232, 236, 248),
        .title = UI_RGB565(120, 132, 152),
        .caption = UI_RGB565(160, 172, 192),
        .dot_idle = UI_RGB565(64, 72, 88),
        .dot_active = UI_RGB565(72, 160, 255),
        .glow = UI_RGB565(72, 160, 255),
        .mod_locked = UI_RGB565(32, 96, 192),
        .lock = UI_RGB565(232, 236, 248),
    },
    .tile_radius = 20,
    .tile_down_border = 3,
    .glow_width = 28,
    .glow_spread = 2,
    .glow_opa = 120,
    .mod_outline_width = 4,
    .mod_outline_pad = 2,
    .lock = {
        .w = 26,
        .h = 32,
        .shackle_w = 18,
        .shackle_h = 20,
        .stroke = 4,
        .body_h = 20,
        .radius = 4,
        .inset = 18,
    },
    .media = {
        .gap = 14,
        .label_inset = 16,
    },
    .mod_poll_ms = 50,
    .dim_opa = LV_OPA_40,
    .stale_opa = LV_OPA_60,
    .widget = {
        .value_dy = 16,
        .caption_dy = 30,
        .bar_h = 6,
        .bar_bottom = 12,
        .bar_radius = 3,
        .redraw_ms = 100,
    },
    .dash = {
        .row_gap = 8,
        .radius = 10,
        .pad = 20,
        .value_x = 360,
        .bar_x = 600,
        .bar_h = 12,
    },
    /* Only this band slides, so a slide redraws the band and leaves the strips above and below it alone. It is
     * taller than the grid by 20 px each side, and a pressed tile's glow reaches 17 px (glow_width / 2 + 1 +
     * glow_spread). */
    .band = { .x = 0, .y = 56, .w = TD_LCD_H_RES, .h = 504 },
    .grid = { .x = 28, .y = 76, .w = 968, .h = 464 },
    .gap = 16,
    .title_x = 36,
    .title_y = 14,
    .dot_y = 580,
    .dot_pitch = 34,
    .dot_size = 12,
    .dot_active_extra = 11,
    .bounce_px = 48,
    .anim_settle_ms = 260,
    .anim_bounce_out_ms = 110,
    .anim_bounce_back_ms = 150,
    .anim_press_ms = 120,
    .anim_dot_ms = 160,
    .font_tile = &lv_font_montserrat_48,
    .font_title = &lv_font_montserrat_28,
    .font_caption = &lv_font_montserrat_28,
};

static const lv_style_prop_t k_tile_transition_props[] = {
    LV_STYLE_BG_COLOR, LV_STYLE_BORDER_WIDTH, LV_STYLE_SHADOW_WIDTH, LV_STYLE_SHADOW_OPA, 0,
};
static const lv_style_prop_t k_dot_transition_props[] = {
    LV_STYLE_BG_COLOR, LV_STYLE_TRANSFORM_WIDTH, 0,
};

static lv_style_transition_dsc_t s_tile_transition;
static lv_style_transition_dsc_t s_dot_transition;
static ui_styles_t s_styles;
static bool s_styles_ready;

lv_color_t ui_color(uint16_t rgb565)
{
    return lv_color_make((uint8_t)(((rgb565 >> 11) & 0x1F) << 3), (uint8_t)(((rgb565 >> 5) & 0x3F) << 2),
                         (uint8_t)((rgb565 & 0x1F) << 3));
}

static void init_tile_styles(ui_styles_t *st)
{
    const ui_theme_t *t = &ui_theme;
    lv_style_transition_dsc_init(&s_tile_transition, k_tile_transition_props, lv_anim_path_ease_out,
                                 t->anim_press_ms, 0, NULL);

    lv_style_init(&st->tile);
    lv_style_set_radius(&st->tile, t->tile_radius);
    lv_style_set_bg_opa(&st->tile, LV_OPA_COVER);
    lv_style_set_bg_color(&st->tile, ui_color(t->color.tile));
    lv_style_set_border_opa(&st->tile, LV_OPA_COVER);
    lv_style_set_border_width(&st->tile, 0);
    lv_style_set_border_color(&st->tile, ui_color(t->color.glow));
    lv_style_set_shadow_width(&st->tile, 0);
    lv_style_set_shadow_spread(&st->tile, t->glow_spread);
    lv_style_set_shadow_color(&st->tile, ui_color(t->color.glow));
    lv_style_set_shadow_opa(&st->tile, LV_OPA_TRANSP);
    lv_style_set_pad_all(&st->tile, 0);
    lv_style_set_transition(&st->tile, &s_tile_transition);

    lv_style_init(&st->tile_down);
    lv_style_set_bg_color(&st->tile_down, ui_color(t->color.tile_down));
    lv_style_set_border_width(&st->tile_down, t->tile_down_border);
    lv_style_set_shadow_width(&st->tile_down, t->glow_width);
    lv_style_set_shadow_opa(&st->tile_down, t->glow_opa);

    lv_style_init(&st->tile_label);
    lv_style_set_text_font(&st->tile_label, t->font_tile);
    lv_style_set_text_color(&st->tile_label, ui_color(t->color.text));

    lv_style_init(&st->tile_caption);
    lv_style_set_text_font(&st->tile_caption, t->font_caption);
    lv_style_set_text_color(&st->tile_caption, ui_color(t->color.caption));
    lv_style_set_text_align(&st->tile_caption, LV_TEXT_ALIGN_CENTER);
}

static void init_text_and_dot_styles(ui_styles_t *st)
{
    const ui_theme_t *t = &ui_theme;
    lv_style_transition_dsc_init(&s_dot_transition, k_dot_transition_props, lv_anim_path_ease_out, t->anim_dot_ms, 0,
                                 NULL);

    lv_style_init(&st->title);
    lv_style_set_text_font(&st->title, t->font_title);
    lv_style_set_text_color(&st->title, ui_color(t->color.title));

    lv_style_init(&st->dot);
    lv_style_set_radius(&st->dot, LV_RADIUS_CIRCLE);
    lv_style_set_bg_opa(&st->dot, LV_OPA_COVER);
    lv_style_set_bg_color(&st->dot, ui_color(t->color.dot_idle));
    lv_style_set_border_width(&st->dot, 0);
    lv_style_set_transition(&st->dot, &s_dot_transition);

    lv_style_init(&st->dot_on);
    lv_style_set_bg_color(&st->dot_on, ui_color(t->color.dot_active));
    lv_style_set_transform_width(&st->dot_on, t->dot_active_extra);
}

/* LVGL ranks a state's styles by the state's value, CHECKED 4, PRESSED 128, USER_1 4096, so a press still shows the
 * pressed fill over a locked tile. The outline is not in the tile's transition list, so the ring appears in one
 * frame. */
static void init_mod_styles(ui_styles_t *st)
{
    const ui_theme_t *t = &ui_theme;

    lv_style_init(&st->mod_one_shot);
    lv_style_set_outline_width(&st->mod_one_shot, t->mod_outline_width);
    lv_style_set_outline_pad(&st->mod_one_shot, t->mod_outline_pad);
    lv_style_set_outline_color(&st->mod_one_shot, ui_color(t->color.glow));
    lv_style_set_outline_opa(&st->mod_one_shot, LV_OPA_COVER);

    lv_style_init(&st->mod_locked);
    lv_style_set_bg_color(&st->mod_locked, ui_color(t->color.mod_locked));

    lv_style_init(&st->lock_shackle);
    lv_style_set_radius(&st->lock_shackle, LV_RADIUS_CIRCLE);
    lv_style_set_bg_opa(&st->lock_shackle, LV_OPA_TRANSP);
    lv_style_set_border_opa(&st->lock_shackle, LV_OPA_COVER);
    lv_style_set_border_width(&st->lock_shackle, t->lock.stroke);
    lv_style_set_border_color(&st->lock_shackle, ui_color(t->color.lock));
    lv_style_set_border_side(&st->lock_shackle, LV_BORDER_SIDE_TOP | LV_BORDER_SIDE_LEFT | LV_BORDER_SIDE_RIGHT);

    lv_style_init(&st->lock_body);
    lv_style_set_radius(&st->lock_body, t->lock.radius);
    lv_style_set_bg_opa(&st->lock_body, LV_OPA_COVER);
    lv_style_set_bg_color(&st->lock_body, ui_color(t->color.lock));
    lv_style_set_border_width(&st->lock_body, 0);
}

/* The dim style sits in LV_STATE_USER_2, which ranks above PRESSED, so a dimmed tile stays dim under a finger. */
static void init_widget_styles(ui_styles_t *st)
{
    const ui_theme_t *t = &ui_theme;

    lv_style_init(&st->tile_dim);
    lv_style_set_opa(&st->tile_dim, t->dim_opa);

    lv_style_init(&st->widget_bar);
    lv_style_set_radius(&st->widget_bar, t->widget.bar_radius);
    lv_style_set_bg_opa(&st->widget_bar, LV_OPA_COVER);
    lv_style_set_bg_color(&st->widget_bar, ui_color(t->color.glow));
    lv_style_set_border_width(&st->widget_bar, 0);

    lv_style_init(&st->dash_row);
    lv_style_set_radius(&st->dash_row, t->dash.radius);
    lv_style_set_bg_opa(&st->dash_row, LV_OPA_COVER);
    lv_style_set_bg_color(&st->dash_row, ui_color(t->color.tile));
    lv_style_set_border_width(&st->dash_row, 0);
    lv_style_set_pad_all(&st->dash_row, 0);
}

const ui_styles_t *ui_styles(void)
{
    if (!s_styles_ready) {
        init_tile_styles(&s_styles);
        init_text_and_dot_styles(&s_styles);
        init_mod_styles(&s_styles);
        init_widget_styles(&s_styles);
        s_styles_ready = true;
    }
    return &s_styles;
}

int16_t ui_dot_x(uint8_t index, uint8_t count)
{
    const int span = (count > 0 ? count - 1 : 0) * ui_theme.dot_pitch;
    return (int16_t)((TD_LCD_H_RES - span) / 2 + index * ui_theme.dot_pitch);
}
