#include <string.h>
#include "ui_tile_keys.h"
#include "keys.h"
#include "ui_theme.h"
#include "ui_tile_action.h"

/* The console, a swipe and the idle code write the engine too, so the subjects follow keys_get_state() and not the
 * tiles' own taps. lv_subject_set_int notifies only on change, so a quiet poll redraws nothing. */
static lv_subject_t *s_mod[KEY_MOD_COUNT];

static void sync_mods(void)
{
    const key_engine_state_t st = keys_get_state();
    for (unsigned m = 0; m < KEY_MOD_COUNT; m++) {
        lv_subject_set_int(s_mod[m], st.mods[m].state);
    }
}

static void on_poll(lv_timer_t *timer)
{
    (void)timer;
    sync_mods();
}

static void apply_look(lv_observer_t *observer, lv_subject_t *subject)
{
    lv_obj_t *tile = lv_observer_get_target_obj(observer);
    const int32_t state = lv_subject_get_int(subject);
    lv_obj_set_state(tile, LV_STATE_USER_1, state == KEY_MOD_ONE_SHOT);
    lv_obj_set_state(tile, LV_STATE_CHECKED, state == KEY_MOD_LOCKED);
}

static void apply_glyph(lv_observer_t *observer, lv_subject_t *subject)
{
    lv_obj_set_hidden(lv_observer_get_target_obj(observer), lv_subject_get_int(subject) != KEY_MOD_LOCKED);
}

/* Not clickable, so a tap that lands on the glyph still reaches the tile, and on_press in ui_deck.c finds it. */
static void add_glyph_part(lv_obj_t *tile, lv_subject_t *subject, const lv_style_t *style, int32_t w, int32_t h,
                           int32_t x_ofs, int32_t y_ofs)
{
    lv_obj_t *part = lv_obj_create(tile);
    lv_obj_remove_style_all(part);
    lv_obj_add_style(part, style, 0);
    lv_obj_set_size(part, w, h);
    lv_obj_set_clickable(part, false);
    lv_obj_set_scrollable(part, false);
    lv_obj_align(part, LV_ALIGN_RIGHT_MID, x_ofs, y_ofs);
    lv_subject_add_observer_obj(subject, apply_glyph, part, NULL);
}

void ui_tile_keys_start(void)
{
    for (unsigned m = 0; m < KEY_MOD_COUNT; m++) {
        s_mod[m] = lv_subject_create(LV_SUBJECT_TYPE_INT);
    }
    sync_mods();
    lv_timer_create(on_poll, ui_theme.mod_poll_ms, NULL);
}

static const struct {
    const char *tile_id;
    const char *glyph;
} k_icons[] = {
    { "prev", LV_SYMBOL_PREV },
    { "playpause", LV_SYMBOL_PLAY LV_SYMBOL_PAUSE },
    { "next", LV_SYMBOL_NEXT },
    { "mute", LV_SYMBOL_MUTE },
    { "voldown", LV_SYMBOL_VOLUME_MID },
    { "volup", LV_SYMBOL_VOLUME_MAX },
};

static const char *icon_for(const ui_tile_t *tile)
{
    for (size_t i = 0; i < sizeof k_icons / sizeof k_icons[0]; i++) {
        if (strcmp(k_icons[i].tile_id, tile->id) == 0) {
            return k_icons[i].glyph;
        }
    }
    return NULL;
}

static void dress_media(lv_obj_t *obj, lv_obj_t *label, const char *glyph)
{
    const ui_styles_t *st = ui_styles();
    const ui_theme_t *t = &ui_theme;

    lv_obj_add_style(label, &st->tile_caption, 0);
    lv_obj_set_size(label, lv_obj_get_width(obj) - 2 * t->media.label_inset, lv_font_get_line_height(t->font_caption));
    lv_label_set_long_mode(label, LV_LABEL_LONG_MODE_DOTS);
    lv_obj_set_style_text_align(label, LV_TEXT_ALIGN_CENTER, 0);
    lv_obj_align(label, LV_ALIGN_CENTER, 0, t->media.caption_dy);

    lv_obj_t *icon = lv_label_create(obj);
    lv_label_set_text_static(icon, glyph);
    lv_obj_add_style(icon, &st->tile_label, 0);
    lv_obj_align(icon, LV_ALIGN_CENTER, 0, t->media.icon_dy);
}

void ui_tile_keys_dress(lv_obj_t *obj, lv_obj_t *label, const ui_tile_t *tile)
{
    const ui_action_t action = ui_tile_action(tile);
    if (action.kind == UI_ACTION_CONSUMER) {
        const char *glyph = icon_for(tile);
        if (glyph != NULL) {
            dress_media(obj, label, glyph);
        }
        return;
    }
    if (action.kind != UI_ACTION_MOD) {
        return;
    }
    const ui_styles_t *st = ui_styles();
    const ui_theme_t *t = &ui_theme;
    lv_subject_t *subject = s_mod[action.mod];

    lv_obj_add_style(obj, &st->mod_one_shot, LV_STATE_USER_1);
    lv_obj_add_style(obj, &st->mod_locked, LV_STATE_CHECKED);
    lv_subject_add_observer_obj(subject, apply_look, obj, NULL);

    add_glyph_part(obj, subject, &st->lock_shackle, t->lock.shackle_w, t->lock.shackle_h,
                   -(t->lock.inset + (t->lock.w - t->lock.shackle_w) / 2), (t->lock.shackle_h - t->lock.h) / 2);
    add_glyph_part(obj, subject, &st->lock_body, t->lock.w, t->lock.body_h, -t->lock.inset,
                   (t->lock.h - t->lock.body_h) / 2);
}

static void click_key(void *ctx, uint8_t usage)
{
    (void)ctx;
    keys_tap(usage);
}

static void click_mod(void *ctx, key_mod_t mod)
{
    (void)ctx;
    keys_mod(mod);
}

static void click_consumer(void *ctx, uint16_t usage)
{
    (void)ctx;
    keys_consumer(usage);
}

void ui_tile_keys_click(const ui_tile_t *tile)
{
    static const ui_action_ops_t ops = { .key = click_key, .mod = click_mod, .consumer = click_consumer };
    if (ui_tile_click(tile, &ops, NULL)) {
        sync_mods();
    }
}

void ui_tile_keys_page_changed(void)
{
    keys_clear_one_shot();
    sync_mods();
}
