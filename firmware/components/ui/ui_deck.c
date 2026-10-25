#include "ui.h"
#include "ui_priv.h"
#include <stdatomic.h>
#include "board_pins.h"
#include "display.h"
#include "esp_log.h"
#include "lvgl.h"
#include "ui_pager.h"
#include "ui_theme.h"

static const char *TAG = "ui";

static const ui_pager_cfg_t k_pager_cfg = { .page_w = TD_LCD_H_RES, .slop = 16, .swipe = 96 };

/* The pager, the objects and the press bookkeeping belong to the LVGL task, and ui_goto() borrows them under the
 * adapter lock. The atomics are what the console task reads without that lock. track_x is the position last set,
 * because lv_obj_get_x reads coordinates that only change when LVGL next lays out the screen. */
static struct {
    ui_pager_t pager;
    const ui_page_t *pages;
    uint8_t page_count;
    lv_obj_t *track;
    int32_t track_x;
    lv_obj_t *title;
    uint8_t title_page;
    lv_obj_t *dots[UI_PAGES_MAX];
    lv_obj_t *pressed_obj;
    bool slide_changes_page;
    uint32_t slide_start_ms;
    uint32_t slide_start_frames;
    atomic_bool built;
    atomic_uint page;
    _Atomic(const char *) pressed;
    atomic_uint taps;
    atomic_bool animating;
    atomic_uint transitions;
    atomic_uint transition_ms;
    atomic_uint transition_frames;
    atomic_uint last_ms;
    atomic_uint last_frames;
} s;

static const ui_tile_t *tile_of(lv_obj_t *obj)
{
    return lv_obj_get_user_data(obj);
}

/* The title belongs to whichever page is nearer the middle of the screen, so it changes halfway through a slide
 * instead of naming the new page over the old one. */
static void sync_title(int32_t track_x)
{
    const int32_t page_w = s.pager.cfg.page_w;
    int32_t nearest = (page_w / 2 - track_x) / page_w;
    nearest = nearest >= s.page_count ? s.page_count - 1 : nearest;
    if (nearest != s.title_page) {
        s.title_page = (uint8_t)nearest;
        lv_label_set_text_static(s.title, s.pages[nearest].title);
    }
}

static void set_track_x(void *track, int32_t x)
{
    s.track_x = x;
    lv_obj_set_x(track, x);
    sync_title(x);
}

static void show_page(uint8_t index)
{
    atomic_store(&s.page, index);
    for (uint8_t i = 0; i < s.page_count; i++) {
        if (i == index) {
            lv_obj_add_state(s.dots[i], LV_STATE_CHECKED);
        } else {
            lv_obj_remove_state(s.dots[i], LV_STATE_CHECKED);
        }
    }
}

static void clear_pressed(void)
{
    if (s.pressed_obj != NULL) {
        lv_obj_remove_state(s.pressed_obj, LV_STATE_PRESSED);
        s.pressed_obj = NULL;
    }
    atomic_store(&s.pressed, "");
}

/* The totals go in before animating clears, so a reader that sees animating false reads them whole. */
static void slide_done(bool cut)
{
    const uint32_t ms = lv_tick_elaps(s.slide_start_ms);
    const uint32_t frames = display_frame_count() - s.slide_start_frames;
    if (!cut && s.slide_changes_page) {
        atomic_fetch_add(&s.transitions, 1);
        atomic_fetch_add(&s.transition_ms, ms);
        atomic_fetch_add(&s.transition_frames, frames);
        atomic_store(&s.last_ms, ms);
        atomic_store(&s.last_frames, frames);
    }
    atomic_store(&s.animating, false);
    ESP_LOGI(TAG, "%s page=%u ms=%u frames=%u", cut ? "slide cut" : s.slide_changes_page ? "page shown" : "bounce done",
             (unsigned)atomic_load(&s.page), (unsigned)ms, (unsigned)frames);
}

static void on_slide_completed(lv_anim_t *anim)
{
    (void)anim;
    slide_done(false);
}

static void begin_slide(lv_anim_t *anim, bool changes_page)
{
    s.slide_changes_page = changes_page;
    s.slide_start_ms = lv_tick_get();
    s.slide_start_frames = display_frame_count();
    atomic_store(&s.animating, true);
    lv_anim_set_var(anim, s.track);
    lv_anim_set_exec_cb(anim, set_track_x);
    lv_anim_set_path_cb(anim, lv_anim_path_ease_out);
    lv_anim_set_completed_cb(anim, on_slide_completed);
    if (lv_anim_start(anim) == NULL) {
        set_track_x(s.track, ui_pager_rest_x(&s.pager));
        atomic_store(&s.animating, false);
    }
}

static void start_slide(void)
{
    const int32_t to = ui_pager_rest_x(&s.pager);
    if (s.track_x == to) {
        return;
    }
    lv_anim_t anim;
    lv_anim_init(&anim);
    lv_anim_set_values(&anim, s.track_x, to);
    lv_anim_set_duration(&anim, ui_theme.anim_settle_ms);
    begin_slide(&anim, true);
}

/* A swipe with no page to go to nudges the pages toward it, the way the finger went, and lets them spring back. */
static void start_bounce(int8_t direction)
{
    const int32_t rest = ui_pager_rest_x(&s.pager);
    lv_anim_t anim;
    lv_anim_init(&anim);
    lv_anim_set_values(&anim, rest, rest + direction * ui_theme.bounce_px);
    lv_anim_set_duration(&anim, ui_theme.anim_bounce_out_ms);
    lv_anim_set_reverse_duration(&anim, ui_theme.anim_bounce_back_ms);
    begin_slide(&anim, false);
}

/* Jumps a running slide to its end, so a new touch starts from a page at rest. */
static void finish_slide(void)
{
    if (!atomic_load(&s.animating)) {
        return;
    }
    lv_anim_delete(s.track, set_track_x);
    set_track_x(s.track, ui_pager_rest_x(&s.pager));
    slide_done(true);
}

static lv_point_t pointer_point(void)
{
    lv_point_t p = { 0, 0 };
    lv_indev_t *indev = lv_indev_active();
    if (indev != NULL) {
        lv_indev_get_point(indev, &p);
    }
    return p;
}

/* A touch that lands on a moving page stops it, but never counts as a tap, because the tile under the finger is
 * not where it will end up. It can still swipe. */
static void on_press(lv_event_t *e)
{
    const bool caught = atomic_load(&s.animating);
    finish_slide();
    clear_pressed();

    const lv_point_t p = pointer_point();
    ui_pager_press(&s.pager, (int16_t)p.x, (int16_t)p.y);

    lv_obj_t *target = lv_event_get_target_obj(e);
    const ui_tile_t *tile = tile_of(target);
    if (tile == NULL) {
        return;
    }
    if (caught) {
        lv_obj_remove_state(target, LV_STATE_PRESSED);
        return;
    }
    s.pressed_obj = target;
    atomic_store(&s.pressed, tile->id);
}

/* The pages slide on the move that makes the swipe, and not after the lift, so the deck answers while the finger
 * is still moving. They do not follow the finger. Each frame takes about 65 ms, LVGL reads one touch sample per
 * frame while it draws, and a drawn drag would slow every injected swipe to a crawl. */
static void on_pressing(lv_event_t *e)
{
    (void)e;
    const lv_point_t p = pointer_point();
    const ui_press_t before = s.pager.press;
    const ui_pager_move_t m = ui_pager_move(&s.pager, (int16_t)p.x, (int16_t)p.y);
    if (before == UI_PRESS_TAP && m.press != UI_PRESS_TAP) {
        clear_pressed();
    }
    if (m.changed) {
        show_page(m.to);
        start_slide();
    } else if (m.blocked != 0) {
        start_bounce(m.blocked);
    }
}

/* pressed_obj is set only for a press that began on a tile, was not caught, and has not left the slop radius, so a
 * tap that lifts with it set is a click. */
static void on_release(lv_event_t *e)
{
    lv_obj_t *tile_obj = s.pressed_obj;
    const bool tap = ui_pager_release(&s.pager);
    clear_pressed();
    if (!tap || tile_obj == NULL || lv_event_get_code(e) != LV_EVENT_RELEASED) {
        return;
    }
    const unsigned taps = atomic_fetch_add(&s.taps, 1) + 1;
    ESP_LOGI(TAG, "click taps=%u tile=%s", taps, tile_of(tile_obj)->id);
}

static void build_tile(lv_obj_t *parent, const ui_page_t *page, const ui_tile_t *tile)
{
    const ui_styles_t *st = ui_styles();
    const ui_rect_t r = ui_tile_rect(page, tile, ui_theme.grid, ui_theme.gap);

    lv_obj_t *obj = lv_obj_create(parent);
    lv_obj_remove_style_all(obj);
    lv_obj_add_style(obj, &st->tile, 0);
    lv_obj_add_style(obj, &st->tile_down, LV_STATE_PRESSED);
    lv_obj_set_pos(obj, r.x, r.y - ui_theme.band.y);
    lv_obj_set_size(obj, r.w, r.h);
    lv_obj_set_scrollable(obj, false);
    lv_obj_set_event_bubble(obj, true);
    lv_obj_set_user_data(obj, (void *)tile);

    lv_obj_t *label = lv_label_create(obj);
    lv_label_set_text_static(label, tile->label);
    lv_obj_add_style(label, &st->tile_label, 0);
    lv_obj_center(label);
}

static void build_page(uint8_t index)
{
    const ui_page_t *page = &s.pages[index];
    lv_obj_t *obj = lv_obj_create(s.track);
    lv_obj_remove_style_all(obj);
    lv_obj_set_size(obj, s.pager.cfg.page_w, ui_theme.band.h);
    lv_obj_set_pos(obj, index * s.pager.cfg.page_w, 0);
    lv_obj_set_scrollable(obj, false);
    lv_obj_set_event_bubble(obj, true);

    for (uint8_t i = 0; i < page->tile_count; i++) {
        build_tile(obj, page, &page->tiles[i]);
    }
}

static void build_dots(lv_obj_t *screen)
{
    const ui_styles_t *st = ui_styles();
    for (uint8_t i = 0; i < s.page_count; i++) {
        lv_obj_t *dot = lv_obj_create(screen);
        lv_obj_remove_style_all(dot);
        lv_obj_add_style(dot, &st->dot, 0);
        lv_obj_add_style(dot, &st->dot_on, LV_STATE_CHECKED);
        lv_obj_set_size(dot, ui_theme.dot_size, ui_theme.dot_size);
        lv_obj_set_pos(dot, ui_dot_x(i, s.page_count) - ui_theme.dot_size / 2, ui_theme.dot_y - ui_theme.dot_size / 2);
        lv_obj_set_scrollable(dot, false);
        lv_obj_set_clickable(dot, false);
        s.dots[i] = dot;
    }
}

/* A page that fails ui_page_valid ends the list there, so a bad table shows the pages before it. */
static uint8_t count_valid_pages(const ui_page_t *pages, size_t count)
{
    uint8_t valid = 0;
    while (valid < count && valid < UI_PAGES_MAX && ui_page_valid(&pages[valid])) {
        valid++;
    }
    if (valid < count) {
        ESP_LOGE(TAG, "page %u is invalid or past the limit of %d, so the deck shows %u pages", valid, UI_PAGES_MAX,
                 valid);
    }
    return valid;
}

void ui_build(void)
{
    size_t count = 0;
    s.pages = ui_builtin_pages(&count);
    s.page_count = count_valid_pages(s.pages, count);
    if (s.page_count == 0) {
        return;
    }
    ui_styles();
    ui_pager_init(&s.pager, &k_pager_cfg, s.page_count);
    atomic_store(&s.pressed, "");

    lv_obj_t *screen = lv_screen_active();
    lv_obj_set_scrollable(screen, false);
    lv_obj_set_press_lock(screen, true);
    lv_obj_set_style_bg_color(screen, ui_color(ui_theme.color.bg), 0);
    lv_obj_set_style_bg_opa(screen, LV_OPA_COVER, 0);
    lv_obj_add_event_cb(screen, on_press, LV_EVENT_PRESSED, NULL);
    lv_obj_add_event_cb(screen, on_pressing, LV_EVENT_PRESSING, NULL);
    lv_obj_add_event_cb(screen, on_release, LV_EVENT_RELEASED, NULL);
    lv_obj_add_event_cb(screen, on_release, LV_EVENT_PRESS_LOST, NULL);

    lv_obj_t *band = lv_obj_create(screen);
    lv_obj_remove_style_all(band);
    lv_obj_set_pos(band, ui_theme.band.x, ui_theme.band.y);
    lv_obj_set_size(band, ui_theme.band.w, ui_theme.band.h);
    lv_obj_set_scrollable(band, false);
    lv_obj_set_event_bubble(band, true);

    s.track = lv_obj_create(band);
    lv_obj_remove_style_all(s.track);
    lv_obj_set_pos(s.track, 0, 0);
    lv_obj_set_size(s.track, s.pager.cfg.page_w * s.page_count, ui_theme.band.h);
    lv_obj_set_scrollable(s.track, false);
    lv_obj_set_event_bubble(s.track, true);

    for (uint8_t i = 0; i < s.page_count; i++) {
        build_page(i);
    }

    s.title = lv_label_create(screen);
    lv_label_set_text_static(s.title, s.pages[0].title);
    lv_obj_add_style(s.title, &ui_styles()->title, 0);
    lv_obj_set_pos(s.title, ui_theme.title_x, ui_theme.title_y);

    build_dots(screen);
    show_page(0);
    atomic_store(&s.built, true);
}

bool ui_ready(void)
{
    return atomic_load(&s.built);
}

void ui_get_state(ui_state_t *out)
{
    const uint8_t page = (uint8_t)atomic_load(&s.page);
    *out = (ui_state_t){
        .page = page,
        .pages = s.page_count,
        .title = s.pages[page].title,
        .pressed_tile = atomic_load(&s.pressed),
        .taps = atomic_load(&s.taps),
        .animating = atomic_load(&s.animating),
        .transitions = atomic_load(&s.transitions),
        .transition_ms = atomic_load(&s.transition_ms),
        .transition_frames = atomic_load(&s.transition_frames),
        .last_ms = atomic_load(&s.last_ms),
        .last_frames = atomic_load(&s.last_frames),
    };
}

bool ui_goto(uint8_t page)
{
    if (page >= s.page_count) {
        return false;
    }
    finish_slide();
    clear_pressed();
    const uint8_t from = s.pager.index;
    ui_pager_goto(&s.pager, page);
    if (page != from) {
        show_page(page);
        start_slide();
    }
    return true;
}

bool ui_find_tile(const char *id, uint8_t *page, ui_rect_t *rect)
{
    for (uint8_t i = 0; i < s.page_count; i++) {
        const ui_tile_t *tile = ui_page_find_tile(&s.pages[i], id);
        if (tile != NULL) {
            *page = i;
            *rect = ui_tile_rect(&s.pages[i], tile, ui_theme.grid, ui_theme.gap);
            return true;
        }
    }
    return false;
}
