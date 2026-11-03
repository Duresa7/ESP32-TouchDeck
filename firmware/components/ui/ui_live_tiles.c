#include "ui.h"
#include "ui_priv.h"
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "esp32s3/rom/tjpgd.h"
#include "esp_heap_caps.h"
#include "esp_log.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"
#include "freertos/task.h"
#include "td_asset.h"
#include "ui_bar.h"
#include "ui_graph.h"
#include "ui_live_bar.h"
#include "ui_now_playing.h"
#include "ui_theme.h"

static const char *TAG = "live";

#define JPEG_WORK_BYTES 4096

/* LVGL refuses an image whose data_size is under stride * h, so `rgb565` holds a spare row after its h rows of w
 * pixels for a window ui_np_cover_crop cuts at the full row stride. */
typedef struct {
    uint16_t w;
    uint16_t h;
    uint16_t rgb565[];
} cover_t;

typedef enum {
    VIEW_GRAPH,
    VIEW_BAR,
    VIEW_NOW_PLAYING,
} view_kind_t;

/* lv_line keeps a pointer to `points` rather than a copy. */
typedef struct {
    lv_obj_t *area;
    lv_obj_t *line;
    lv_obj_t *label;
    lv_obj_t *marker;
    int16_t area_w;
    int16_t area_h;
    lv_point_precise_t points[UI_LIVE_SERIES_MAX];
} graph_view_t;

typedef struct {
    lv_obj_t *glyph;
    lv_obj_t *label;
    lv_obj_t *value;
    lv_obj_t *track;
    lv_obj_t *fill;
    ui_bar_layout_t layout;
    ui_live_bar_t live;
    bool set;
    bool fresh;
    bool muted;
} bar_view_t;

typedef struct {
    lv_obj_t *placeholder;
    lv_obj_t *art;
    lv_obj_t *title;
    lv_obj_t *artist;
    lv_obj_t *controls[UI_NP_PARTS_MAX];
    ui_np_layout_t layout;
    int16_t h;
    ui_np_state_t tap_state;
    cover_t *shown;
    lv_image_dsc_t dsc[2];
    uint8_t dsc_next;
} np_view_t;

typedef struct {
    view_kind_t kind;
    const ui_tile_t *tile;
    const char *page;
    bool built;
    int16_t stream_slot[UI_LIVE_TILE_STREAMS];
    union {
        graph_view_t graph;
        bar_view_t bar;
        np_view_t np;
    };
} view_t;

static struct {
    SemaphoreHandle_t lock;
    ui_live_table_t table;
    ui_live_slot_t *slots;
    cover_t **undrawn;
    view_t *views;
    uint16_t view_count;
    int16_t dragging;
    lv_timer_t *timer;
    void (*input)(const char *page, const char *tile, double value, bool end);
    void (*cancel)(void);
} s = { .dragging = -1 };

static ui_live_slot_t s_view_slots[UI_LIVE_TILE_STREAMS];
static uint32_t s_view_generation;

static uint32_t now_ms(void)
{
    return (uint32_t)(esp_timer_get_time() / 1000);
}

static void *alloc(size_t n, size_t size)
{
    void *p = heap_caps_calloc(n, size, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
    return p != NULL ? p : calloc(n, size);
}

static void op_lock(void *ctx)
{
    (void)ctx;
    xSemaphoreTake(s.lock, portMAX_DELAY);
}

static void op_unlock(void *ctx)
{
    (void)ctx;
    xSemaphoreGive(s.lock);
}

static ui_live_result_t op_pin(void *ctx, const char *id)
{
    (void)ctx;
    const td_asset_result_t result = td_asset_pin(id);
    return result == TD_ASSET_OK ? UI_LIVE_OK : result == TD_ASSET_MEMORY ? UI_LIVE_MEMORY : UI_LIVE_ASSET;
}

static void op_unpin(void *ctx, const char *id)
{
    (void)ctx;
    td_asset_unpin(id);
}

typedef struct {
    const uint8_t *jpeg;
    size_t len;
    size_t at;
    cover_t *cover;
} decode_t;

static UINT jpeg_in(JDEC *jd, BYTE *buf, UINT want)
{
    decode_t *d = jd->device;
    const size_t left = d->len - d->at;
    const UINT n = want < left ? want : (UINT)left;
    if (buf != NULL) {
        memcpy(buf, d->jpeg + d->at, n);
    }
    d->at += n;
    return n;
}

/* TJpgDec hands over RGB888 blocks. Sleeping a tick after each row of them lets the idle tasks feed the task
 * watchdog. */
static UINT jpeg_out(JDEC *jd, void *bitmap, JRECT *rect)
{
    decode_t *d = jd->device;
    const BYTE *rgb = bitmap;
    for (UINT y = rect->top; y <= rect->bottom; y++) {
        uint16_t *row = d->cover->rgb565 + (size_t)y * d->cover->w + rect->left;
        for (UINT x = rect->left; x <= rect->right; x++, rgb += 3) {
            *row++ = (uint16_t)(((rgb[0] & 0xf8) << 8) | ((rgb[1] & 0xfc) << 3) | (rgb[2] >> 3));
        }
    }
    if (rect->right + 1u >= d->cover->w) {
        vTaskDelay(1);
    }
    return 1;
}

static ui_live_result_t op_decode(void *ctx, const char *id, void **pixels)
{
    (void)ctx;
    uint8_t *jpeg = heap_caps_malloc(TD_ASSET_JPEG_MAX_BYTES, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
    void *work = heap_caps_malloc(JPEG_WORK_BYTES, MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT);
    decode_t d = { .jpeg = jpeg };
    ui_live_result_t result = UI_LIVE_MEMORY;
    if (jpeg != NULL && work != NULL) {
        result = td_asset_read(id, jpeg, TD_ASSET_JPEG_MAX_BYTES, &d.len) == TD_ASSET_OK ? UI_LIVE_OK : UI_LIVE_ASSET;
    }
    JDEC jd = { 0 };
    if (result == UI_LIVE_OK) {
        const JRESULT res = jd_prepare(&jd, jpeg_in, work, JPEG_WORK_BYTES, &d);
        if (res == JDR_MEM1 || res == JDR_MEM2) {
            result = UI_LIVE_MEMORY;
        } else if (res != JDR_OK || jd.width == 0 || jd.height == 0 || jd.width > TD_ASSET_IMAGE_MAX ||
                   jd.height > TD_ASSET_IMAGE_MAX) {
            result = UI_LIVE_ASSET;
        }
    }
    if (result == UI_LIVE_OK) {
        const size_t bytes = sizeof *d.cover + (size_t)jd.width * (jd.height + 1u) * sizeof d.cover->rgb565[0];
        d.cover = heap_caps_malloc(bytes, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
        if (d.cover == NULL) {
            result = UI_LIVE_MEMORY;
        } else {
            d.cover->w = (uint16_t)jd.width;
            d.cover->h = (uint16_t)jd.height;
            if (jd_decomp(&jd, jpeg_out, 0) != JDR_OK) {
                heap_caps_free(d.cover);
                d.cover = NULL;
                result = UI_LIVE_ASSET;
            }
        }
    }
    heap_caps_free(work);
    heap_caps_free(jpeg);
    if (result == UI_LIVE_OK) {
        *pixels = d.cover;
    } else {
        ESP_LOGW(TAG, "cover %.12s refused: %s", id, ui_live_reason(result));
    }
    return result;
}

static void *op_show(void *ctx, uint16_t index, void *pixels)
{
    (void)ctx;
    cover_t *old = s.undrawn[index];
    s.undrawn[index] = pixels;
    return old;
}

static void op_discard(void *ctx, void *pixels)
{
    (void)ctx;
    heap_caps_free(pixels);
}

static const ui_live_ops_t k_ops = {
    .lock = op_lock,
    .unlock = op_unlock,
    .pin = op_pin,
    .unpin = op_unpin,
    .decode = op_decode,
    .show = op_show,
    .discard = op_discard,
};

void ui_set_input_hooks(void (*input)(const char *page, const char *tile, double value, bool end),
                        void (*cancel)(void))
{
    s.input = input;
    s.cancel = cancel;
}

static void cancel_input(void)
{
    if (s.cancel != NULL) {
        s.cancel();
    }
}

static void drop_capture(void)
{
    if (s.dragging >= 0) {
        ui_bar_cancel(&s.views[s.dragging].bar.live.bar);
        s.dragging = -1;
        cancel_input();
    }
}

static double latest(const ui_live_slot_t *slot)
{
    return slot->count > 0 ? slot->samples[(slot->head + slot->count - 1) % UI_LIVE_SERIES_MAX] : 0;
}

static const ui_live_slot_t *stream(const view_t *v, const char *id)
{
    for (uint8_t k = 0; k < v->tile->stream_count && k < UI_LIVE_TILE_STREAMS; k++) {
        if (v->stream_slot[k] >= 0 && strcmp(v->tile->streams[k].id, id) == 0) {
            return &s_view_slots[k];
        }
    }
    return NULL;
}

static void set_text(lv_obj_t *label, const char *text)
{
    if (strcmp(lv_label_get_text(label), text) != 0) {
        lv_label_set_text(label, text);
    }
}

static bool copy_slots(view_t *v, bool even_clean, cover_t **undrawn)
{
    *undrawn = NULL;
    bool dirty = even_clean;
    xSemaphoreTake(s.lock, portMAX_DELAY);
    s_view_generation = s.table.generation;
    for (uint8_t k = 0; k < UI_LIVE_TILE_STREAMS && !dirty; k++) {
        dirty = v->stream_slot[k] >= 0 && s.table.slots[v->stream_slot[k]].dirty;
    }
    for (uint8_t k = 0; k < UI_LIVE_TILE_STREAMS && dirty; k++) {
        const int16_t i = v->stream_slot[k];
        if (i < 0) {
            continue;
        }
        s_view_slots[k] = s.table.slots[i];
        s.table.slots[i].dirty = false;
        if (v->tile->streams[k].type == UI_STREAM_IMAGE && s.undrawn[i] != NULL) {
            *undrawn = s.undrawn[i];
            s.undrawn[i] = NULL;
        }
    }
    xSemaphoreGive(s.lock);
    return dirty;
}

static void draw_graph(graph_view_t *o)
{
    const ui_live_slot_t *slot = &s_view_slots[0];
    double samples[UI_LIVE_SERIES_MAX];
    ui_graph_point_t points[UI_LIVE_SERIES_MAX];
    const uint8_t count = ui_graph_points(samples, ui_live_samples(slot, samples), UI_LIVE_SERIES_MAX, o->area_w, o->area_h,
                                          points);
    for (uint8_t i = 0; i < count; i++) {
        o->points[i] = (lv_point_precise_t){ points[i].x, points[i].y };
    }
    lv_line_set_points(o->line, o->points, count);
    lv_obj_set_hidden(o->line, count < 2);
    const lv_opa_t opa = slot->stale ? ui_theme.stale_opa : LV_OPA_COVER;
    lv_obj_set_style_opa(o->area, opa, 0);
    lv_obj_set_style_opa(o->label, opa, 0);
    lv_obj_set_hidden(o->marker, slot->set && !slot->stale);
}

static void draw_bar(bar_view_t *b)
{
    const ui_theme_t *t = &ui_theme;
    const double shown = ui_bar_shown(&b->live.bar);
    char text[8] = "--";
    if (b->fresh) {
        snprintf(text, sizeof text, "%d", (int)lround(fmin(fmax(shown, 0), 1) * 100));
    }
    set_text(b->value, text);
    set_text(b->glyph, b->muted ? LV_SYMBOL_MUTE : LV_SYMBOL_VOLUME_MAX);
    const int16_t fill = b->set ? ui_bar_fill(&b->layout, shown) : 0;
    lv_obj_set_width(b->fill, fill);
    lv_obj_set_hidden(b->fill, fill == 0);
    lv_obj_set_style_bg_color(b->fill, ui_color(b->muted ? t->color.ink_2 : t->color.accent), 0);
    const lv_opa_t opa = b->fresh ? LV_OPA_COVER : t->stale_opa;
    lv_obj_t *const parts[] = { b->glyph, b->label, b->value, b->track, b->fill };
    for (size_t i = 0; i < sizeof parts / sizeof parts[0]; i++) {
        lv_obj_set_style_opa(parts[i], opa, 0);
    }
}

static void update_bar(view_t *v)
{
    bar_view_t *b = &v->bar;
    const ui_live_slot_t *value = stream(v, "value");
    const ui_live_slot_t *muted = stream(v, "muted");
    b->set = value != NULL && value->set;
    b->fresh = b->set && !value->stale;
    b->muted = muted != NULL && muted->set && latest(muted) != 0;
    ui_live_bar_feed(&b->live, s_view_generation, value);
    if (!b->fresh && (b->live.bar.pressed || b->live.bar.phase != UI_BAR_LIVE)) {
        if (s.dragging >= 0 && &s.views[s.dragging] == v) {
            drop_capture();
        } else {
            ui_bar_cancel(&b->live.bar);
        }
    }
    draw_bar(b);
}

static void show_cover(view_t *v, cover_t *cover)
{
    np_view_t *n = &v->np;
    if (n->art == NULL) {
        heap_caps_free(cover);
        return;
    }
    lv_image_dsc_t *dsc = &n->dsc[n->dsc_next];
    n->dsc_next ^= 1;
    const ui_rect_t crop = ui_np_cover_crop((int16_t)cover->w, (int16_t)cover->h, n->layout.art);
    const uint32_t stride = cover->w * sizeof cover->rgb565[0];
    *dsc = (lv_image_dsc_t){
        .header = { .magic = LV_IMAGE_HEADER_MAGIC, .cf = LV_COLOR_FORMAT_RGB565, .w = (uint32_t)crop.w,
                    .h = (uint32_t)crop.h, .stride = stride },
        .data_size = stride * (uint32_t)crop.h,
        .data = (const uint8_t *)(cover->rgb565 + (size_t)crop.y * cover->w + crop.x),
    };
    cover_t *old = n->shown;
    lv_image_set_src(n->art, dsc);
    n->shown = cover;
    heap_caps_free(old);
    ESP_LOGI(TAG, "cover %s shown t=%u", v->tile->id, (unsigned)now_ms());
}

static void clear_cover(np_view_t *n)
{
    if (n->shown != NULL) {
        lv_image_set_src(n->art, NULL);
        heap_caps_free(n->shown);
        n->shown = NULL;
    }
}

static void update_now_playing(view_t *v, cover_t *cover)
{
    const ui_theme_t *t = &ui_theme;
    np_view_t *n = &v->np;
    const ui_live_slot_t *title = stream(v, "title");
    const ui_live_slot_t *artist = stream(v, "artist");
    const ui_live_slot_t *art = stream(v, "cover");
    const ui_live_slot_t *state = stream(v, "state");
    const bool known = state != NULL && state->set;
    const ui_np_state_t st = known ? ui_np_state(latest(state)) : UI_NP_IDLE;
    const bool idle = known && st == UI_NP_IDLE;
    const bool named = known && !idle;
    bool stale = !known;
    const ui_live_slot_t *const all[] = { title, artist, art, state };
    for (size_t i = 0; i < sizeof all / sizeof all[0]; i++) {
        stale = stale || (all[i] != NULL && all[i]->set && all[i]->stale);
    }
    n->tap_state = known && !state->stale ? st : UI_NP_IDLE;

    if (cover != NULL && art != NULL && art->image[0] != '\0') {
        show_cover(v, cover);
    } else {
        heap_caps_free(cover);
    }
    if (art == NULL || art->image[0] == '\0') {
        clear_cover(n);
    }
    if (n->art != NULL) {
        lv_obj_set_hidden(n->art, n->shown == NULL || idle);
    }

    const bool controls = named && n->layout.controls;
    const int32_t block = named ? t->now_playing.title_h + t->now_playing.artist_h : t->now_playing.title_h;
    const int32_t top = n->layout.has_art ? n->layout.art.y : UI_NP_PAD;
    const int32_t row = controls ? n->layout.zones[0].y : n->h;
    int32_t y = !controls ? (n->h - block) / 2 : top + block <= row ? top : UI_NP_PAD + (row - UI_NP_PAD - block) / 2;
    y = y > 0 ? y : 0;
    const int32_t title_line = lv_font_get_line_height(t->font_value);
    const int32_t artist_line = lv_font_get_line_height(t->font_sub);
    lv_obj_set_y(n->title, y + (t->now_playing.title_h - title_line) / 2);
    set_text(n->title, named ? (title != NULL && title->set ? title->text : "--") : idle ? "Nothing playing" : "--");
    lv_obj_set_style_text_color(n->title, ui_color(idle ? t->color.ink_2 : t->color.ink), 0);
    lv_obj_set_hidden(n->artist, !named);
    lv_obj_set_y(n->artist, y + t->now_playing.title_h + (t->now_playing.artist_h - artist_line) / 2);
    set_text(n->artist, named && artist != NULL && artist->set ? artist->text : "");

    static const char *const k_glyphs[UI_NP_PARTS_MAX] = { LV_SYMBOL_PREV, LV_SYMBOL_PAUSE, LV_SYMBOL_NEXT };
    for (uint8_t i = 0; i < UI_NP_PARTS_MAX; i++) {
        if (n->controls[i] == NULL) {
            continue;
        }
        lv_obj_set_hidden(n->controls[i], !controls);
        lv_obj_t *glyph = lv_obj_get_child(n->controls[i], 0);
        set_text(glyph, n->layout.parts[i] == UI_PART_PLAY_PAUSE && st == UI_NP_PAUSED ? LV_SYMBOL_PLAY : k_glyphs[i]);
    }

    const lv_opa_t opa = stale ? t->stale_opa : LV_OPA_COVER;
    lv_obj_t *const parts[] = { n->placeholder, n->art, n->title, n->artist, n->controls[0], n->controls[1],
                                n->controls[2] };
    for (size_t i = 0; i < sizeof parts / sizeof parts[0]; i++) {
        if (parts[i] != NULL) {
            lv_obj_set_style_opa(parts[i], opa, 0);
        }
    }
}

static void update(view_t *v, cover_t *cover)
{
    switch (v->kind) {
    case VIEW_GRAPH:
        if (v->stream_slot[0] >= 0) {
            draw_graph(&v->graph);
        }
        break;
    case VIEW_BAR:
        update_bar(v);
        break;
    case VIEW_NOW_PLAYING:
        update_now_playing(v, cover);
        return;
    }
    heap_caps_free(cover);
}

/* Expiry and offline marks are taken under the lock, and the dirty slots are copied out and drawn after it, so the
 * link task never waits on LVGL. */
static void on_redraw(lv_timer_t *timer)
{
    (void)timer;
    if (s.lock == NULL || s.views == NULL) {
        return;
    }
    const bool offline = !ui_link_online();
    const uint32_t now = now_ms();
    xSemaphoreTake(s.lock, portMAX_DELAY);
    if (offline) {
        ui_live_offline(&s.table);
    }
    ui_live_expire(&s.table, now);
    xSemaphoreGive(s.lock);
    for (uint16_t i = 0; i < s.view_count; i++) {
        view_t *v = &s.views[i];
        if (!v->built) {
            continue;
        }
        cover_t *cover = NULL;
        if (copy_slots(v, false, &cover)) {
            update(v, cover);
        }
        if (v->kind == VIEW_BAR && ui_bar_tick(&v->bar.live.bar, now)) {
            draw_bar(&v->bar);
        }
    }
}

static bool is_live(const ui_tile_t *tile)
{
    return tile->stream_count > 0 &&
           (tile->render == UI_RENDER_GRAPH || tile->render == UI_RENDER_BAR || tile->render == UI_RENDER_NOW_PLAYING);
}

static uint16_t count_views(const td_layout_t *layout)
{
    uint16_t count = 0;
    for (uint8_t p = 0; p < layout->page_count; p++) {
        for (uint8_t t = 0; layout->pages[p].kind == UI_PAGE_GRID && t < layout->pages[p].tile_count; t++) {
            count = (uint16_t)(count + is_live(&layout->pages[p].tiles[t]));
        }
    }
    return count;
}

static void map_views(view_t *views, const td_layout_t *layout)
{
    uint16_t n = 0;
    for (uint8_t p = 0; p < layout->page_count; p++) {
        for (uint8_t t = 0; layout->pages[p].kind == UI_PAGE_GRID && t < layout->pages[p].tile_count; t++) {
            const ui_tile_t *tile = &layout->pages[p].tiles[t];
            if (!is_live(tile)) {
                continue;
            }
            view_t *v = &views[n++];
            v->kind = tile->render == UI_RENDER_BAR           ? VIEW_BAR
                      : tile->render == UI_RENDER_NOW_PLAYING ? VIEW_NOW_PLAYING
                                                              : VIEW_GRAPH;
            v->tile = tile;
            v->page = layout->pages[p].id;
            for (uint8_t k = 0; k < UI_LIVE_TILE_STREAMS; k++) {
                const ui_live_slot_t *slot =
                    k < tile->stream_count ? ui_live_find(&s.table, tile, tile->streams[k].id) : NULL;
                v->stream_slot[k] = slot != NULL ? (int16_t)(slot - s.table.slots) : -1;
            }
        }
    }
}

void ui_live_begin(const td_layout_t *layout)
{
    if (s.lock == NULL) {
        s.lock = xSemaphoreCreateMutex();
    }
    drop_capture();
    cancel_input();
    size_t want = ui_live_streams(layout->pages, layout->page_count);
    want = want > UI_LIVE_SLOTS_MAX ? UI_LIVE_SLOTS_MAX : want;
    const uint16_t view_count = count_views(layout);
    ui_live_slot_t *slots = want > 0 ? alloc(want, sizeof *slots) : NULL;
    cover_t **undrawn = want > 0 ? alloc(want, sizeof *undrawn) : NULL;
    view_t *views = view_count > 0 ? alloc(view_count, sizeof *views) : NULL;
    if (slots == NULL || undrawn == NULL || views == NULL) {
        free(slots);
        free(undrawn);
        free(views);
        slots = NULL;
        undrawn = NULL;
        views = NULL;
    }
    xSemaphoreTake(s.lock, portMAX_DELAY);
    ui_live_slot_t *old_slots = s.slots;
    cover_t **old_undrawn = s.undrawn;
    const uint16_t old_count = s.table.count;
    s.slots = slots;
    s.undrawn = undrawn;
    ui_live_bind(&s.table, slots, (uint16_t)(slots != NULL ? want : 0), layout->builtin ? "builtin" : layout->sha256,
                 layout->pages, layout->page_count);
    if (views != NULL) {
        map_views(views, layout);
    }
    xSemaphoreGive(s.lock);
    for (uint16_t i = 0; old_undrawn != NULL && i < old_count; i++) {
        heap_caps_free(old_undrawn[i]);
    }
    for (uint16_t i = 0; i < s.view_count; i++) {
        if (s.views[i].kind == VIEW_NOW_PLAYING) {
            heap_caps_free(s.views[i].np.shown);
        }
    }
    free(s.views);
    s.views = views;
    s.view_count = views != NULL ? view_count : 0;
    free(old_slots);
    free(old_undrawn);
    if (s.table.dropped > 0 || (want > 0 && slots == NULL)) {
        ESP_LOGW(TAG, "%u live streams have no slot", (unsigned)(s.table.dropped + (slots == NULL ? want : 0)));
    }
    if (s.timer == NULL) {
        s.timer = lv_timer_create(on_redraw, ui_theme.widget.redraw_ms, NULL);
    }
}

void ui_live_unpin_dropped(void)
{
    if (s.lock != NULL) {
        ui_live_release(&s.table, &k_ops);
    }
}

static lv_obj_t *box(lv_obj_t *parent, ui_rect_t r)
{
    lv_obj_t *obj = lv_obj_create(parent);
    lv_obj_remove_style_all(obj);
    lv_obj_set_pos(obj, r.x, r.y);
    lv_obj_set_size(obj, r.w, r.h);
    lv_obj_set_clickable(obj, false);
    lv_obj_set_scrollable(obj, false);
    return obj;
}

static lv_obj_t *text(lv_obj_t *parent, const lv_font_t *font, uint16_t color, lv_text_align_t align, int32_t x,
                      int32_t y, int32_t w)
{
    lv_obj_t *label = lv_label_create(parent);
    lv_obj_set_style_text_font(label, font, 0);
    lv_obj_set_style_text_color(label, ui_color(color), 0);
    lv_obj_set_style_text_align(label, align, 0);
    lv_obj_set_pos(label, x, y);
    lv_obj_set_size(label, LV_MAX(w, 0), lv_font_get_line_height(font));
    lv_label_set_long_mode(label, LV_LABEL_LONG_MODE_DOTS);
    lv_label_set_text_static(label, "");
    return label;
}

static lv_obj_t *glyph(lv_obj_t *parent, ui_rect_t r)
{
    lv_obj_t *holder = box(parent, r);
    lv_obj_t *label = lv_label_create(holder);
    lv_obj_set_style_text_font(label, ui_theme.font_glyph, 0);
    lv_obj_set_style_text_color(label, ui_color(ui_theme.color.ink), 0);
    lv_label_set_text_static(label, "");
    lv_obj_center(label);
    return holder;
}

static void build_graph(lv_obj_t *obj, const ui_tile_t *tile, ui_rect_t rect, graph_view_t *o)
{
    const ui_styles_t *st = ui_styles();
    const int32_t pad = ui_theme.graph.pad;
    const int32_t top = ui_theme.graph.top;
    const int32_t line_h = lv_font_get_line_height(ui_theme.font_label);

    lv_obj_t *label = lv_label_create(obj);
    lv_obj_add_style(label, &st->tile_label, 0);
    lv_obj_set_style_text_align(label, LV_TEXT_ALIGN_LEFT, 0);
    lv_obj_set_size(label, rect.w - 2 * pad, line_h);
    lv_label_set_long_mode(label, LV_LABEL_LONG_MODE_DOTS);
    lv_label_set_text(label, tile->label);
    lv_obj_set_pos(label, pad, (top - line_h) / 2);

    lv_obj_t *marker = lv_label_create(obj);
    lv_obj_add_style(marker, &st->tile_label, 0);
    lv_label_set_text_static(marker, "--");
    lv_obj_align(marker, LV_ALIGN_TOP_RIGHT, -pad, (top - line_h) / 2);

    const int16_t w = (int16_t)LV_MAX(rect.w - 2 * pad, 0);
    const int16_t h = (int16_t)LV_MAX(rect.h - top - pad, 0);
    lv_obj_t *area = lv_obj_create(obj);
    lv_obj_remove_style_all(area);
    lv_obj_add_style(area, &st->graph_area, 0);
    lv_obj_set_pos(area, pad, top);
    lv_obj_set_size(area, w, h);
    lv_obj_set_clickable(area, false);
    lv_obj_set_scrollable(area, false);

    lv_obj_t *line = lv_line_create(area);
    lv_obj_remove_style_all(line);
    lv_obj_add_style(line, &st->graph_line, 0);
    lv_obj_set_pos(line, 0, 0);
    lv_obj_set_size(line, w, h);
    lv_obj_set_clickable(line, false);
    lv_obj_set_hidden(line, true);

    *o = (graph_view_t){ .area = area, .line = line, .label = label, .marker = marker, .area_w = w, .area_h = h };
}

static bool declares(const ui_tile_t *tile, const char *id)
{
    for (uint8_t k = 0; k < tile->stream_count; k++) {
        if (strcmp(tile->streams[k].id, id) == 0) {
            return true;
        }
    }
    return false;
}

static void build_bar(lv_obj_t *obj, const ui_tile_t *tile, ui_rect_t rect, bar_view_t *b)
{
    const ui_theme_t *t = &ui_theme;
    b->layout = ui_bar_layout(rect.w, rect.h, declares(tile, "muted"));
    ui_live_bar_init(&b->live, 0);
    const ui_rect_t g = b->layout.glyph;
    const int32_t value_x = rect.w - g.x - t->level.value_w;
    const int32_t label_x = g.x + g.w + t->level.gap;
    const int32_t label_line = lv_font_get_line_height(t->font_label);
    const int32_t value_line = lv_font_get_line_height(t->font_value);

    b->glyph = lv_obj_get_child(glyph(obj, g), 0);
    b->label = text(obj, t->font_label, t->color.ink_2, LV_TEXT_ALIGN_LEFT, label_x,
                    g.y + (t->level.line_h - label_line) / 2, value_x - t->level.gap - label_x);
    lv_label_set_text(b->label, tile->label);
    b->value = text(obj, t->font_value, t->color.ink, LV_TEXT_ALIGN_RIGHT, value_x,
                    g.y + (t->level.line_h - value_line) / 2, t->level.value_w);

    b->track = box(obj, b->layout.track);
    lv_obj_set_style_radius(b->track, t->level.track_radius, 0);
    lv_obj_set_style_bg_color(b->track, ui_color(t->color.track), 0);
    lv_obj_set_style_bg_opa(b->track, LV_OPA_COVER, 0);
    b->fill = box(obj, (ui_rect_t){ b->layout.track.x, b->layout.track.y, 0, b->layout.track.h });
    lv_obj_set_style_radius(b->fill, t->level.track_radius, 0);
    lv_obj_set_style_bg_opa(b->fill, LV_OPA_COVER, 0);
}

static void build_now_playing(lv_obj_t *obj, ui_rect_t rect, np_view_t *n)
{
    const ui_theme_t *t = &ui_theme;
    n->layout = ui_np_layout(rect.w, rect.h);
    n->h = rect.h;
    if (n->layout.has_art) {
        n->placeholder = box(obj, n->layout.art);
        lv_obj_set_style_radius(n->placeholder, t->now_playing.art_radius, 0);
        lv_obj_set_style_bg_color(n->placeholder, ui_color(t->color.track), 0);
        lv_obj_set_style_bg_opa(n->placeholder, LV_OPA_COVER, 0);
        n->art = lv_image_create(obj);
        lv_obj_set_pos(n->art, n->layout.art.x, n->layout.art.y);
        lv_obj_set_size(n->art, n->layout.art.w, n->layout.art.h);
        lv_image_set_inner_align(n->art, LV_IMAGE_ALIGN_CENTER);
        lv_obj_set_style_radius(n->art, t->now_playing.art_radius, 0);
        lv_obj_set_clickable(n->art, false);
        lv_obj_set_hidden(n->art, true);
    }
    n->title = text(obj, t->font_value, t->color.ink, LV_TEXT_ALIGN_LEFT, n->layout.text_x, 0, n->layout.text_w);
    n->artist = text(obj, t->font_sub, t->color.ink_2, LV_TEXT_ALIGN_LEFT, n->layout.text_x, 0, n->layout.text_w);
    for (uint8_t i = 0; n->layout.controls && i < n->layout.part_count; i++) {
        n->controls[i] = glyph(obj, n->layout.zones[i]);
    }
}

void ui_live_build(lv_obj_t *obj, const ui_tile_t *tile, ui_rect_t rect)
{
    view_t *v = NULL;
    for (uint16_t i = 0; i < s.view_count && v == NULL; i++) {
        v = s.views[i].tile == tile ? &s.views[i] : NULL;
    }
    view_t scratch = { .kind = VIEW_GRAPH, .tile = tile, .stream_slot = { -1, -1, -1, -1 } };
    view_t *target = v != NULL ? v : &scratch;
    switch (tile->render) {
    case UI_RENDER_BAR:
        target->kind = VIEW_BAR;
        build_bar(obj, tile, rect, &target->bar);
        break;
    case UI_RENDER_NOW_PLAYING:
        target->kind = VIEW_NOW_PLAYING;
        build_now_playing(obj, rect, &target->np);
        break;
    default:
        target->kind = VIEW_GRAPH;
        build_graph(obj, tile, rect, &target->graph);
        break;
    }
    if (v == NULL) {
        update(target, NULL);
        return;
    }
    v->built = true;
    cover_t *cover = NULL;
    copy_slots(v, true, &cover);
    update(v, cover);
}

static view_t *view_of(const ui_tile_t *tile)
{
    for (uint16_t i = 0; i < s.view_count; i++) {
        if (s.views[i].tile == tile && s.views[i].built) {
            return &s.views[i];
        }
    }
    return NULL;
}

bool ui_live_capture(const ui_tile_t *tile, int16_t x, int16_t y)
{
    drop_capture();
    view_t *v = view_of(tile);
    if (v == NULL || v->kind != VIEW_BAR ||
        ui_bar_hit(&v->bar.layout, x, y, v->bar.fresh, tile->input) != UI_BAR_ZONE_VALUE) {
        return false;
    }
    xSemaphoreTake(s.lock, portMAX_DELAY);
    const uint32_t generation = s.table.generation;
    xSemaphoreGive(s.lock);
    if (!ui_live_bar_press(&v->bar.live, generation, x, y)) {
        return false;
    }
    s.dragging = (int16_t)(v - s.views);
    return true;
}

void ui_live_drag(int16_t x, int16_t y)
{
    if (s.dragging < 0) {
        return;
    }
    view_t *v = &s.views[s.dragging];
    xSemaphoreTake(s.lock, portMAX_DELAY);
    const bool moved =
        ui_live_bar_move(&v->bar.live, s.table.generation, &v->bar.layout, x, y, ui_geo()->slop);
    if (moved && s.input != NULL) {
        s.input(v->page, v->tile->id, v->bar.live.bar.finger, false);
    }
    xSemaphoreGive(s.lock);
    if (moved) {
        draw_bar(&v->bar);
    }
}

void ui_live_drop(void)
{
    if (s.dragging < 0) {
        return;
    }
    view_t *v = &s.views[s.dragging];
    s.dragging = -1;
    xSemaphoreTake(s.lock, portMAX_DELAY);
    const bool ended = ui_live_bar_release(&v->bar.live, s.table.generation, now_ms());
    if (ended && s.input != NULL) {
        s.input(v->page, v->tile->id, v->bar.live.bar.finger, true);
    }
    xSemaphoreGive(s.lock);
    if (ended) {
        draw_bar(&v->bar);
    }
}

const char *ui_live_part(const ui_tile_t *tile, int16_t x, int16_t y)
{
    const view_t *v = view_of(tile);
    if (v == NULL) {
        return NULL;
    }
    if (v->kind == VIEW_BAR) {
        return ui_bar_hit(&v->bar.layout, x, y, v->bar.fresh, tile->input) == UI_BAR_ZONE_MUTE
                   ? ui_part_name(UI_PART_MUTE)
                   : NULL;
    }
    return v->kind == VIEW_NOW_PLAYING ? ui_part_name(ui_np_hit(&v->np.layout, x, y, v->np.tap_state)) : NULL;
}

void ui_live_select(void)
{
    if (s.lock == NULL) {
        return;
    }
    xSemaphoreTake(s.lock, portMAX_DELAY);
    ui_live_session(&s.table, now_ms());
    xSemaphoreGive(s.lock);
    cancel_input();
    ui_live_release(&s.table, &k_ops);
}

ui_live_result_t ui_live_update(const ui_live_set_t *set, uint32_t *retry_ms)
{
    if (s.lock == NULL) {
        return UI_LIVE_LAYOUT;
    }
    return ui_live_apply(&s.table, set, now_ms(), retry_ms, &k_ops);
}
