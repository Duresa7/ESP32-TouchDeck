#include "ui.h"
#include "ui_priv.h"
#include <stdatomic.h>
#include <stdio.h>
#include <string.h>
#include "display.h"
#include "esp_log.h"
#include "esp_lv_adapter.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"
#include "lvgl.h"
#include "ui_pager.h"
#include "ui_theme.h"
#include "ui_tile_keys.h"

static const char *TAG = "ui";
static const char *LAYOUT_TAG = "layout";

#define LOCK_TIMEOUT_MS 3000

/*
 * The pager, the objects and the press bookkeeping belong to the LVGL task, and ui_goto() and a reload borrow them
 * under the adapter lock. The layout lock guards the layout pointer, the page table and `info`, which console
 * readers copy without the adapter lock. A reload takes the adapter lock and then the layout lock, and nothing takes
 * them the other way round. reload_lock lets one reload run at a time. The atomics are what the console task reads
 * without either lock. track_x is the position last set, because lv_obj_get_x reads coordinates that only change when
 * LVGL next lays out the screen.
 */
static struct {
    SemaphoreHandle_t layout_lock;
    SemaphoreHandle_t reload_lock;
    const td_layout_t *layout;
    ui_layout_info_t info;
    const td_layout_t *boot;
    ui_layout_info_t boot_info;
    char orientation[12];
    bool screen_set_up;
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
    atomic_int pressed;
    atomic_uint taps;
    atomic_bool animating;
    atomic_uint transitions;
    atomic_uint transition_ms;
    atomic_uint transition_frames;
    atomic_uint last_ms;
    atomic_uint last_frames;
} s = { .pressed = -1 };

static void make_locks(void)
{
    if (s.layout_lock == NULL) {
        s.layout_lock = xSemaphoreCreateMutex();
        s.reload_lock = xSemaphoreCreateMutex();
    }
}

static void layout_lock(void)
{
    xSemaphoreTake(s.layout_lock, portMAX_DELAY);
}

static void layout_unlock(void)
{
    xSemaphoreGive(s.layout_lock);
}

static const ui_tile_t *tile_of(lv_obj_t *obj)
{
    return lv_obj_get_user_data(obj);
}

/* The pressed tile as page * 256 + its index on the page, so a reader can name it under the layout lock. */
static int tile_ref(const ui_tile_t *tile)
{
    for (uint8_t p = 0; p < s.page_count; p++) {
        const ui_page_t *page = &s.pages[p];
        if (page->tile_count > 0 && tile >= page->tiles && tile < page->tiles + page->tile_count) {
            return p * 256 + (int)(tile - page->tiles);
        }
    }
    return -1;
}

/* The title belongs to whichever page is nearer the middle of the screen, so it changes halfway through a slide
 * instead of naming the new page over the old one. */
static void sync_title(int32_t track_x)
{
    const int32_t page_w = s.pager.cfg.page_w;
    int32_t nearest = (page_w / 2 - track_x) / page_w;
    nearest = nearest >= s.page_count ? s.page_count - 1 : nearest;
    nearest = nearest < 0 ? 0 : nearest;
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
    const bool changed = index != atomic_load(&s.page);
    atomic_store(&s.page, index);
    if (changed) {
        ui_tile_keys_page_changed();
    }
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
    atomic_store(&s.pressed, -1);
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
    lv_anim_set_values(&anim, rest, rest + direction * ui_geo()->bounce_px);
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
    if (s.page_count == 0) {
        return;
    }
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
    atomic_store(&s.pressed, tile_ref(tile));
}

/* The pages slide on the move that makes the swipe, and not after the lift, so the deck answers while the finger
 * is still moving. They do not follow the finger. Each frame takes about 65 ms, LVGL reads one touch sample per
 * frame while it draws, and a drawn drag would slow every injected swipe to a crawl. */
static void on_pressing(lv_event_t *e)
{
    (void)e;
    if (s.page_count == 0) {
        return;
    }
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
    if (s.page_count == 0) {
        return;
    }
    lv_obj_t *tile_obj = s.pressed_obj;
    const bool tap = ui_pager_release(&s.pager);
    clear_pressed();
    if (!tap || tile_obj == NULL || lv_event_get_code(e) != LV_EVENT_RELEASED) {
        return;
    }
    const ui_tile_t *tile = tile_of(tile_obj);
    const unsigned taps = atomic_fetch_add(&s.taps, 1) + 1;
    ESP_LOGI(TAG, "click taps=%u tile=%s", taps, tile->id);
    if (ui_widget_tap_blocked(tile)) {
        ESP_LOGI(TAG, "offline %s", tile->id);
        return;
    }
    const int ref = tile_ref(tile);
    ui_tile_keys_click(ref >= 0 ? &s.pages[ref / 256] : NULL, tile);
}

/* An icon above the label, with the label in the caption font. */
static void stack_icon(lv_obj_t *label, lv_obj_t *icon, int32_t icon_h, int32_t tile_w)
{
    const int32_t caption_h = lv_font_get_line_height(ui_theme.font_caption);
    const int32_t gap = ui_theme.media.gap;

    lv_obj_add_style(label, &ui_styles()->tile_caption, 0);
    lv_obj_set_size(label, tile_w - 2 * ui_theme.media.label_inset, caption_h);
    lv_label_set_long_mode(label, LV_LABEL_LONG_MODE_CLIP);
    lv_obj_align(label, LV_ALIGN_CENTER, 0, (icon_h + gap) / 2);

    lv_obj_set_clickable(icon, false);
    lv_obj_align(icon, LV_ALIGN_CENTER, 0, -(caption_h + gap) / 2);
}

static void build_tile(lv_obj_t *parent, const ui_page_t *page, const ui_tile_t *tile)
{
    const ui_styles_t *st = ui_styles();
    const ui_geometry_t *g = ui_geo();
    const ui_rect_t r = ui_tile_rect(page, tile, g->grid, g->gap);

    lv_obj_t *obj = lv_obj_create(parent);
    lv_obj_remove_style_all(obj);
    lv_obj_add_style(obj, &st->tile, 0);
    lv_obj_add_style(obj, &st->tile_down, LV_STATE_PRESSED);
    lv_obj_set_pos(obj, r.x, r.y - g->band.y);
    lv_obj_set_size(obj, r.w, r.h);
    lv_obj_set_scrollable(obj, false);
    lv_obj_set_event_bubble(obj, true);
    lv_obj_set_user_data(obj, (void *)tile);
    ui_widget_track(obj, tile);
    if (tile->action.type == TD_ACTION_WIDGET) {
        ui_widget_build(obj, tile, r);
        return;
    }

    lv_obj_t *label = lv_label_create(obj);
    lv_label_set_text_static(label, tile->label);
    const char *glyph = ui_icon_symbol(tile->icon);
    const lv_image_dsc_t *image = glyph == NULL ? ui_icon_image(tile->icon) : NULL;
    if (glyph != NULL) {
        lv_obj_t *icon = lv_label_create(obj);
        lv_label_set_text_static(icon, glyph);
        lv_obj_add_style(icon, &st->tile_label, 0);
        stack_icon(label, icon, lv_font_get_line_height(ui_theme.font_tile), r.w);
    } else if (image != NULL) {
        lv_obj_t *icon = lv_image_create(obj);
        lv_image_set_src(icon, image);
        stack_icon(label, icon, image->header.h, r.w);
    } else {
        lv_obj_add_style(label, &st->tile_label, 0);
        lv_obj_center(label);
    }

    ui_tile_keys_dress(obj, tile);
}

static void build_page(uint8_t index)
{
    const ui_page_t *page = &s.pages[index];
    lv_obj_t *obj = lv_obj_create(s.track);
    lv_obj_remove_style_all(obj);
    lv_obj_set_size(obj, s.pager.cfg.page_w, ui_geo()->band.h);
    lv_obj_set_pos(obj, index * s.pager.cfg.page_w, 0);
    lv_obj_set_scrollable(obj, false);
    lv_obj_set_event_bubble(obj, true);

    if (page->kind == UI_PAGE_DASHBOARD) {
        ui_dashboard_build(obj, page);
    }
    for (uint8_t i = 0; i < page->tile_count; i++) {
        build_tile(obj, page, &page->tiles[i]);
    }
}

static void build_dots(lv_obj_t *screen)
{
    const ui_styles_t *st = ui_styles();
    const ui_geometry_t *g = ui_geo();
    for (uint8_t i = 0; i < s.page_count; i++) {
        lv_obj_t *dot = lv_obj_create(screen);
        lv_obj_remove_style_all(dot);
        lv_obj_add_style(dot, &st->dot, 0);
        lv_obj_add_style(dot, &st->dot_on, LV_STATE_CHECKED);
        lv_obj_set_size(dot, g->dot_size, g->dot_size);
        lv_obj_set_pos(dot, ui_dot_x(i, s.page_count) - g->dot_size / 2, g->dot_y - g->dot_size / 2);
        lv_obj_set_scrollable(dot, false);
        lv_obj_set_clickable(dot, false);
        s.dots[i] = dot;
    }
}

/* A page that fails ui_page_valid ends the list there, so a bad table shows the pages before it. The parser already
 * refuses such a layout, so this only guards the drawing. */
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

/* The screen's own look and its input callbacks are set once. A rebuild only replaces the objects under it. */
static void set_up_screen(void)
{
    if (s.screen_set_up) {
        return;
    }
    s.screen_set_up = true;
    ui_styles();
    lv_obj_t *screen = lv_screen_active();
    lv_obj_set_scrollable(screen, false);
    lv_obj_set_press_lock(screen, true);
    lv_obj_set_style_bg_color(screen, ui_color(ui_theme.color.bg), 0);
    lv_obj_set_style_bg_opa(screen, LV_OPA_COVER, 0);
    lv_obj_add_event_cb(screen, on_press, LV_EVENT_PRESSED, NULL);
    lv_obj_add_event_cb(screen, on_pressing, LV_EVENT_PRESSING, NULL);
    lv_obj_add_event_cb(screen, on_release, LV_EVENT_RELEASED, NULL);
    lv_obj_add_event_cb(screen, on_release, LV_EVENT_PRESS_LOST, NULL);
}

/* Cancels the press and the slide in flight and deletes every object of the old layout. */
static void tear_down(void)
{
    lv_obj_t *screen = lv_screen_active();
    if (s.track != NULL) {
        lv_anim_delete(s.track, NULL);
    }
    atomic_store(&s.animating, false);
    s.pressed_obj = NULL;
    lv_indev_reset(NULL, NULL);
    lv_obj_clean(screen);
    s.track = NULL;
    s.title = NULL;
    memset(s.dots, 0, sizeof s.dots);
}

static void build_objects(void)
{
    const ui_geometry_t *g = ui_geo();
    lv_obj_t *screen = lv_screen_active();
    lv_obj_t *band = lv_obj_create(screen);
    lv_obj_remove_style_all(band);
    lv_obj_set_pos(band, g->band.x, g->band.y);
    lv_obj_set_size(band, g->band.w, g->band.h);
    lv_obj_set_scrollable(band, false);
    lv_obj_set_event_bubble(band, true);

    s.track = lv_obj_create(band);
    lv_obj_remove_style_all(s.track);
    lv_obj_set_pos(s.track, 0, 0);
    lv_obj_set_size(s.track, s.pager.cfg.page_w * s.page_count, g->band.h);
    lv_obj_set_scrollable(s.track, false);
    lv_obj_set_event_bubble(s.track, true);
    s.track_x = 0;

    for (uint8_t i = 0; i < s.page_count; i++) {
        build_page(i);
    }

    s.title = lv_label_create(screen);
    s.title_page = 0;
    lv_label_set_text_static(s.title, s.pages[0].title);
    lv_obj_add_style(s.title, &ui_styles()->title, 0);
    lv_obj_set_pos(s.title, g->title_x, g->title_y);

    build_dots(screen);
}

/*
 * Swaps a parsed layout onto the screen. The caller holds the adapter lock and the reload lock, and has run
 * ui_icon_prepare for it. The old layout's arena is freed only after the new objects exist, and never when it is the
 * built-in layout.
 */
static void load_locked(const td_layout_t *layout, const ui_layout_info_t *info, int64_t started_us)
{
    set_up_screen();
    tear_down();
    ui_tile_keys_start();
    ui_widget_begin();

    const uint8_t count = count_valid_pages(layout->pages, layout->page_count);
    layout_lock();
    td_layout_t *old = td_layout_replace(&s.layout, layout);
    s.pages = layout->pages;
    s.page_count = count;
    s.info = *info;
    atomic_store(&s.pressed, -1);
    atomic_store(&s.page, 0);
    layout_unlock();

    const ui_geometry_t *g = ui_geo();
    const ui_pager_cfg_t cfg = { .page_w = g->screen.w, .slop = g->slop, .swipe = g->swipe };
    ui_pager_init(&s.pager, &cfg, count > 0 ? count : 1);
    if (count > 0) {
        build_objects();
        show_page(0);
    }
    td_layout_free(old);

    const uint32_t ms = (uint32_t)((esp_timer_get_time() - started_us) / 1000);
    layout_lock();
    s.info.load_ms = ms;
    layout_unlock();
    atomic_store(&s.built, true);
    ESP_LOGI(LAYOUT_TAG, "loaded %s %s pages=%u tiles=%u icons=%u/%u in %u ms", info->from_storage ? "storage" : "builtin",
             layout->name, layout->page_count, layout->tile_count, info->icons_loaded, layout->icons_wanted,
             (unsigned)ms);
}

static void fill_info(ui_layout_info_t *info, const td_layout_t *layout, bool from_storage, size_t bytes,
                      ui_icon_counts_t icons)
{
    memset(info, 0, sizeof *info);
    info->from_storage = from_storage;
    snprintf(info->name, sizeof info->name, "%s", layout->name);
    snprintf(info->orientation, sizeof info->orientation, "%s", layout->orientation);
    info->pages = layout->page_count;
    info->tiles = layout->tile_count;
    info->icons_loaded = icons.loaded;
    info->icons_missing = icons.missing;
    info->bytes = (uint32_t)bytes;
}

static void describe_error(char *buf, size_t cap, const td_layout_error_t *err)
{
    snprintf(buf, cap, "%s: %s", err->key, err->message);
}

/* The boot layout is read before display_start and built inside it, so its load_ms adds the two parts and leaves out
 * the display's own start-up between them. */
static int64_t s_boot_prepare_us;

void ui_layout_boot(const char *orientation)
{
    make_locks();
    const int64_t started = esp_timer_get_time();
    snprintf(s.orientation, sizeof s.orientation, "%s", orientation != NULL ? orientation : "landscape");

    char path[48];
    td_layout_path(s.orientation, path, sizeof path);
    td_layout_t *stored = NULL;
    size_t bytes = 0;
    td_layout_error_t err;
    char reason[UI_LAYOUT_ERROR_MAX] = "";
    if (td_layout_load_file(path, &stored, &bytes, &err)) {
        s.boot = stored;
    } else {
        describe_error(reason, sizeof reason, &err);
        ESP_LOGW(LAYOUT_TAG, "%s, using builtin", reason);
        s.boot = td_layout_builtin();
        bytes = 0;
    }
    if (s.boot == NULL) {
        return;
    }
    const ui_icon_counts_t icons = ui_icon_prepare(s.boot, NULL);
    fill_info(&s.boot_info, s.boot, stored != NULL, bytes, icons);
    snprintf(s.boot_info.error, sizeof s.boot_info.error, "%s", reason);
    s_boot_prepare_us = esp_timer_get_time() - started;
}

void ui_build(void)
{
    make_locks();
    if (s.boot == NULL) {
        const int64_t started = esp_timer_get_time();
        s.boot = td_layout_builtin();
        if (s.boot == NULL) {
            ESP_LOGE(LAYOUT_TAG, "the built-in layout does not parse, so the deck shows nothing");
            return;
        }
        fill_info(&s.boot_info, s.boot, false, 0, ui_icon_prepare(s.boot, NULL));
        s_boot_prepare_us = esp_timer_get_time() - started;
    }
    xSemaphoreTake(s.reload_lock, portMAX_DELAY);
    load_locked(s.boot, &s.boot_info, esp_timer_get_time() - s_boot_prepare_us);
    xSemaphoreGive(s.reload_lock);
}

bool ui_layout_reload(bool use_builtin, td_layout_error_t *err)
{
    make_locks();
    xSemaphoreTake(s.reload_lock, portMAX_DELAY);
    const int64_t started = esp_timer_get_time();
    td_layout_t *stored = NULL;
    const td_layout_t *next = NULL;
    size_t bytes = 0;
    if (use_builtin) {
        next = td_layout_builtin();
    } else {
        char path[48];
        td_layout_path(s.orientation[0] != '\0' ? s.orientation : "landscape", path, sizeof path);
        if (td_layout_load_file(path, &stored, &bytes, err)) {
            next = stored;
        }
    }
    if (next == NULL) {
        if (use_builtin) {
            snprintf(err->key, sizeof err->key, "internal.no-memory");
            snprintf(err->message, sizeof err->message, "the built-in layout is not available");
        }
        layout_lock();
        describe_error(s.info.error, sizeof s.info.error, err);
        layout_unlock();
        ESP_LOGW(LAYOUT_TAG, "%s: %s, keeping %s", err->key, err->message, s.info.name);
        xSemaphoreGive(s.reload_lock);
        return false;
    }

    ui_layout_info_t info;
    fill_info(&info, next, stored != NULL, bytes, ui_icon_prepare(next, s.layout));
    if (esp_lv_adapter_lock(LOCK_TIMEOUT_MS) != ESP_OK) {
        td_layout_free(stored);
        snprintf(err->key, sizeof err->key, "display.busy");
        snprintf(err->message, sizeof err->message, "the display stayed busy for %d ms", LOCK_TIMEOUT_MS);
        xSemaphoreGive(s.reload_lock);
        return false;
    }
    load_locked(next, &info, started);
    esp_lv_adapter_unlock();
    xSemaphoreGive(s.reload_lock);
    return true;
}

const char *ui_layout_slot(void)
{
    return s.orientation[0] != '\0' ? s.orientation : "landscape";
}

bool ui_ready(void)
{
    return atomic_load(&s.built);
}

void ui_get_state(ui_state_t *out)
{
    memset(out, 0, sizeof *out);
    layout_lock();
    const uint8_t page = (uint8_t)atomic_load(&s.page);
    out->page = page;
    out->pages = s.page_count;
    if (page < s.page_count) {
        out->tile_count = s.pages[page].tile_count;
        snprintf(out->title, sizeof out->title, "%s", s.pages[page].title);
    }
    const int pressed = atomic_load(&s.pressed);
    if (pressed >= 0 && pressed / 256 < s.page_count && pressed % 256 < s.pages[pressed / 256].tile_count) {
        snprintf(out->pressed_tile, sizeof out->pressed_tile, "%s", s.pages[pressed / 256].tiles[pressed % 256].id);
    }
    layout_unlock();
    out->taps = atomic_load(&s.taps);
    out->animating = atomic_load(&s.animating);
    out->transitions = atomic_load(&s.transitions);
    out->transition_ms = atomic_load(&s.transition_ms);
    out->transition_frames = atomic_load(&s.transition_frames);
    out->last_ms = atomic_load(&s.last_ms);
    out->last_frames = atomic_load(&s.last_frames);
    out->online = ui_link_online();
    out->dimmed = ui_widget_dimmed();
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

bool ui_tile_info(const char *id, ui_tile_info_t *out)
{
    memset(out, 0, sizeof *out);
    layout_lock();
    uint8_t page = 0;
    const ui_tile_t *tile = NULL;
    for (uint8_t i = 0; tile == NULL && i < s.page_count; i++) {
        tile = ui_page_find_tile(&s.pages[i], id);
        page = i;
    }
    if (tile != NULL) {
        out->page = page;
        out->rect = ui_tile_rect(&s.pages[page], tile, ui_geo()->grid, ui_geo()->gap);
        snprintf(out->label, sizeof out->label, "%s", tile->label);
        snprintf(out->action, sizeof out->action, "%s", td_action_name(&tile->action));
        snprintf(out->icon, sizeof out->icon, "%s", tile->icon != NULL ? tile->icon : "");
    }
    layout_unlock();
    return tile != NULL;
}

void ui_layout_info(ui_layout_info_t *out)
{
    layout_lock();
    *out = s.info;
    layout_unlock();
}
