#include "ui_theme.h"
#include "display.h"

/* Every screen token's RGB565 value, pinned so a lossy edit to a macro fails the build. */
_Static_assert(UI_SCREEN_BG == 0x0883, "--screen-bg");
_Static_assert(UI_SCREEN_TILE == 0x1906, "--screen-tile");
_Static_assert(UI_SCREEN_TILE_RAISED == 0x2168, "--screen-tile-raised");
_Static_assert(UI_SCREEN_TRACK == 0x31C9, "--screen-track");
_Static_assert(UI_SCREEN_PAGER_IDLE == 0x632E, "--screen-pager-idle");
_Static_assert(UI_SCREEN_OUTLINE == 0x2966, "--screen-outline");
_Static_assert(UI_SCREEN_INK == 0xF79F, "--screen-ink");
_Static_assert(UI_SCREEN_INK_2 == 0x94D6, "--screen-ink-2");
_Static_assert(UI_SCREEN_ACCENT == 0x5D5F, "--screen-accent");
_Static_assert(UI_SCREEN_LIVE == 0x46D2, "--screen-live");
_Static_assert(UI_SCREEN_SIGNAL == 0xFD84, "--screen-signal");
_Static_assert(UI_SCREEN_DANGER == 0xFACC, "--screen-danger");
_Static_assert(UI_TILE_COLOR_VIOLET == 0x49F8, "--tile-color-violet");
_Static_assert(UI_TILE_COLOR_GREEN == 0x138A, "--tile-color-green");
_Static_assert(UI_TILE_COLOR_BLUE == 0x22F7, "--tile-color-blue");
_Static_assert(UI_TILE_COLOR_RED == 0xB146, "--tile-color-red");
_Static_assert(UI_TILE_COLOR_ORANGE == 0xA2C2, "--tile-color-orange");
_Static_assert(UI_TILE_COLOR_SLATE == 0x632E, "--tile-color-slate");
_Static_assert(UI_TILE_ACCENT_BACKLIGHT == 0x5D5F, "--tile-accent-backlight");
_Static_assert(UI_TILE_ACCENT_VIOLET == 0xA47F, "--tile-accent-violet");
_Static_assert(UI_TILE_ACCENT_CORAL == 0xFC4D, "--tile-accent-coral");
_Static_assert(UI_TILE_ACCENT_MINT == 0x66B8, "--tile-accent-mint");

const ui_theme_t ui_theme = {
    .color = {
        .bg = UI_SCREEN_BG,
        .tile = UI_SCREEN_TILE,
        .tile_raised = UI_SCREEN_TILE_RAISED,
        .track = UI_SCREEN_TRACK,
        .pager_idle = UI_SCREEN_PAGER_IDLE,
        .ink = UI_SCREEN_INK,
        .ink_2 = UI_SCREEN_INK_2,
        .accent = UI_SCREEN_ACCENT,
        .signal = UI_SCREEN_SIGNAL,
    },
    .tile_radius = 18,
    .ring_width = 4,
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
        .gap = 18,
        .label_inset = 16,
    },
    .mod_poll_ms = 50,
    .dim_opa = LV_OPA_40,
    .stale_opa = LV_OPA_60,
    .widget = {
        .value_dy = 18,
        .caption_dy = 26,
        .pad = 24,
        .bar_h = 8,
        .bar_bottom = 24,
        .redraw_ms = 100,
    },
    .dash = {
        .row_gap = 8,
        .radius = 12,
        .pad = 20,
        .bar_h = 8,
    },
    .anim_settle_ms = 260,
    .anim_bounce_out_ms = 110,
    .anim_bounce_back_ms = 150,
    .anim_press_ms = 120,
    .anim_dot_ms = 160,
    .font_legend = &ui_font_medium_44,
    .font_label = &ui_font_medium_26,
    .font_sub = &ui_font_medium_22,
    .font_value = &ui_font_semibold_32,
    .font_big = &ui_font_semibold_40,
    .font_title = &ui_font_nameplate_26,
    .font_icon = &lv_font_montserrat_48,
};

static const lv_style_prop_t k_tile_transition_props[] = {
    LV_STYLE_BG_COLOR, 0,
};
static const lv_style_prop_t k_dot_transition_props[] = {
    LV_STYLE_BG_COLOR, 0,
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

/* A pressed tile rises to the raised fill. No border, glow or shadow: the Device draws flat fills only. */
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
    lv_style_set_pad_all(&st->tile, 0);
    lv_style_set_transition(&st->tile, &s_tile_transition);

    lv_style_init(&st->tile_down);
    lv_style_set_bg_color(&st->tile_down, ui_color(t->color.tile_raised));

    lv_style_init(&st->tile_legend);
    lv_style_set_text_font(&st->tile_legend, t->font_legend);
    lv_style_set_text_color(&st->tile_legend, ui_color(t->color.ink));

    lv_style_init(&st->tile_label);
    lv_style_set_text_font(&st->tile_label, t->font_label);
    lv_style_set_text_color(&st->tile_label, ui_color(t->color.ink_2));
    lv_style_set_text_align(&st->tile_label, LV_TEXT_ALIGN_CENTER);

    lv_style_init(&st->tile_icon);
    lv_style_set_text_font(&st->tile_icon, t->font_icon);
    lv_style_set_text_color(&st->tile_icon, ui_color(t->color.ink));
}

static void init_text_and_dot_styles(ui_styles_t *st)
{
    const ui_theme_t *t = &ui_theme;
    lv_style_transition_dsc_init(&s_dot_transition, k_dot_transition_props, lv_anim_path_ease_out, t->anim_dot_ms, 0,
                                 NULL);

    lv_style_init(&st->title);
    lv_style_set_text_font(&st->title, t->font_title);
    lv_style_set_text_color(&st->title, ui_color(t->color.ink_2));

    lv_style_init(&st->dot);
    lv_style_set_radius(&st->dot, LV_RADIUS_CIRCLE);
    lv_style_set_bg_opa(&st->dot, LV_OPA_COVER);
    lv_style_set_bg_color(&st->dot, ui_color(t->color.pager_idle));
    lv_style_set_border_width(&st->dot, 0);
    lv_style_set_transition(&st->dot, &s_dot_transition);

    lv_style_init(&st->dot_on);
    lv_style_set_bg_color(&st->dot_on, ui_color(t->color.accent));
}

/* LVGL ranks a state's styles by the state's value, CHECKED 4, PRESSED 128, USER_1 4096, so a press still shows the
 * raised fill inside a ring. A border is drawn inside the tile, so it is the design's inset ring. The border is not in
 * the tile's transition list, so the ring appears in one frame. */
static void init_mod_styles(ui_styles_t *st)
{
    const ui_theme_t *t = &ui_theme;

    lv_style_init(&st->mod_one_shot);
    lv_style_set_border_width(&st->mod_one_shot, t->ring_width);
    lv_style_set_border_color(&st->mod_one_shot, ui_color(t->color.accent));

    lv_style_init(&st->mod_locked);
    lv_style_set_border_width(&st->mod_locked, t->ring_width);
    lv_style_set_border_color(&st->mod_locked, ui_color(t->color.ink));

    lv_style_init(&st->lock_shackle);
    lv_style_set_radius(&st->lock_shackle, LV_RADIUS_CIRCLE);
    lv_style_set_bg_opa(&st->lock_shackle, LV_OPA_TRANSP);
    lv_style_set_border_opa(&st->lock_shackle, LV_OPA_COVER);
    lv_style_set_border_width(&st->lock_shackle, t->lock.stroke);
    lv_style_set_border_color(&st->lock_shackle, ui_color(t->color.ink));
    lv_style_set_border_side(&st->lock_shackle, LV_BORDER_SIDE_TOP | LV_BORDER_SIDE_LEFT | LV_BORDER_SIDE_RIGHT);

    lv_style_init(&st->lock_body);
    lv_style_set_radius(&st->lock_body, t->lock.radius);
    lv_style_set_bg_opa(&st->lock_body, LV_OPA_COVER);
    lv_style_set_bg_color(&st->lock_body, ui_color(t->color.ink));
    lv_style_set_border_width(&st->lock_body, 0);
}

/* The dim style sits in LV_STATE_USER_2, which ranks above PRESSED, so a dimmed tile stays dim under a finger. A bar
 * is a flat fill on a flat track, both with round ends. */
static void init_widget_styles(ui_styles_t *st)
{
    const ui_theme_t *t = &ui_theme;

    lv_style_init(&st->tile_dim);
    lv_style_set_opa(&st->tile_dim, t->dim_opa);

    lv_style_init(&st->bar_track);
    lv_style_set_radius(&st->bar_track, LV_RADIUS_CIRCLE);
    lv_style_set_bg_opa(&st->bar_track, LV_OPA_COVER);
    lv_style_set_bg_color(&st->bar_track, ui_color(t->color.track));
    lv_style_set_border_width(&st->bar_track, 0);
    lv_style_set_pad_all(&st->bar_track, 0);

    lv_style_init(&st->bar_fill);
    lv_style_set_radius(&st->bar_fill, LV_RADIUS_CIRCLE);
    lv_style_set_bg_opa(&st->bar_fill, LV_OPA_COVER);
    lv_style_set_bg_color(&st->bar_fill, ui_color(t->color.accent));
    lv_style_set_border_width(&st->bar_fill, 0);

    lv_style_init(&st->widget_value);
    lv_style_set_text_font(&st->widget_value, t->font_big);
    lv_style_set_text_color(&st->widget_value, ui_color(t->color.ink));

    lv_style_init(&st->dash_value);
    lv_style_set_text_font(&st->dash_value, t->font_value);
    lv_style_set_text_color(&st->dash_value, ui_color(t->color.ink));

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

/* Only the band slides, so a slide redraws the band and leaves the strips above and below it alone. It is taller than
 * the grid by 20 px each side. A full-screen band is the screen itself. The screen size is fixed once display_start
 * has run, so the geometry is worked out again only when the size or the full-screen choice differs from the last. */
static ui_geometry_t s_geo;
static bool s_geo_full_screen;
static bool s_want_full_screen;

const ui_geometry_t *ui_geo(void)
{
    const ui_size_t screen = { .w = (int16_t)display_width(), .h = (int16_t)display_height() };
    if (s_geo.screen.w != screen.w || s_geo.screen.h != screen.h || s_geo_full_screen != s_want_full_screen) {
        s_geo = ui_geometry(screen, s_want_full_screen);
        s_geo_full_screen = s_want_full_screen;
    }
    return &s_geo;
}

void ui_geo_set_full_screen(bool full_screen)
{
    s_want_full_screen = full_screen;
    ui_geo();
}
