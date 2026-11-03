#include "ui_priv.h"
#include "ui_gif_player.h"
#include "esp_heap_caps.h"
#include "esp_lv_adapter.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "power.h"
#include "td_asset.h"
#include "src/misc/cache/instance/lv_image_cache.h"
#include "src/misc/cache/instance/lv_image_header_cache.h"
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>

#define GIF_TILES (TD_LAYOUT_MAX_PAGES * TD_GIF_PAGE_TILES)
#define RETRY_MS 1000u

typedef struct {
    char id[TD_ASSET_ID_LEN + 1];
    uint8_t page;
    lv_obj_t *image;
    lv_image_dsc_t descriptors[2];
    unsigned descriptor;
    const uint8_t *front, *ready;
    uint64_t deadline, retry_ms;
    uint16_t width, height;
    uint8_t *poster, *encoded;
    ui_gif_player_t *player;
    uint32_t generation;
    int64_t yielded_us;
    bool failed;
} gif_tile_t;

typedef struct {
    char id[TD_ASSET_ID_LEN + 1];
    uint8_t page;
    uint16_t width, height;
    uint8_t *poster;
} staged_tile_t;

static struct {
    gif_tile_t tiles[GIF_TILES];
    staged_tile_t staged[GIF_TILES];
    unsigned count, staged_count, page;
    uint32_t generation;
    bool sliding, idle, off, dimmed, dim_request, reset, cleared, ui_active;
    TaskHandle_t worker;
    lv_timer_t *timer;
} s = {.reset = true, .cleared = true};
static portMUX_TYPE s_lock = portMUX_INITIALIZER_UNLOCKED;

static void *allocate(void *ctx, size_t bytes)
{
    (void)ctx;
    return heap_caps_malloc(bytes, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
}

static void release(void *ctx, void *ptr) { (void)ctx; heap_caps_free(ptr); }
static uint64_t now_ms(void *ctx) { (void)ctx; return (uint64_t)esp_timer_get_time() / 1000; }

static void notify(void)
{
    if (s.worker) xTaskNotifyGive(s.worker);
}

static void pace(int64_t *yielded_us)
{
    if (esp_timer_get_time() - *yielded_us >= 1000) {
        vTaskDelay(1);
        *yielded_us = esp_timer_get_time();
    }
}

static bool current(gif_tile_t *tile)
{
    portENTER_CRITICAL(&s_lock);
    const bool valid = !s.reset && tile->generation == s.generation;
    portEXIT_CRITICAL(&s_lock);
    return valid;
}

static bool decode_yield(void *ctx)
{
    gif_tile_t *tile = ctx;
    pace(&tile->yielded_us);
    return current(tile);
}

static bool prepare_yield(void *ctx)
{
    pace(ctx);
    return true;
}

static bool in_use(void *ctx, const uint8_t *pixels)
{
    gif_tile_t *tile = ctx;
    portENTER_CRITICAL(&s_lock);
    const bool used = pixels && (tile->front == pixels || tile->ready == pixels);
    portEXIT_CRITICAL(&s_lock);
    return used;
}

static bool present(void *ctx, const uint8_t *pixels, const td_gif_info_t *info, uint64_t deadline)
{
    gif_tile_t *tile = ctx;
    portENTER_CRITICAL(&s_lock);
    const bool accepted = !s.reset && tile->generation == s.generation &&
                          (!tile->ready || tile->ready == pixels);
    if (accepted && tile->front != pixels) {
        tile->width = info->width;
        tile->height = info->height;
        tile->ready = pixels;
        tile->deadline = deadline;
    }
    portEXIT_CRITICAL(&s_lock);
    return accepted;
}

static td_gif_result_t read_asset(const char *id, td_gif_yield_fn yield, void *ctx, uint8_t **out, uint32_t *size)
{
    char path[96];
    snprintf(path, sizeof path, TD_ASSET_DIR "/%c/%s.bin", id[0], id);
    FILE *file = fopen(path, "rb");
    if (!file) return TD_GIF_FORMAT;
    struct stat st;
    if (fstat(fileno(file), &st) || st.st_size <= 0 || st.st_size > TD_GIF_MAX_BYTES) {
        fclose(file);
        return TD_GIF_SIZE;
    }
    const uint32_t bytes = (uint32_t)st.st_size;
    uint8_t *data = allocate(NULL, bytes);
    td_gif_result_t result = data ? TD_GIF_OK : TD_GIF_MEMORY;
    for (uint32_t at = 0; result == TD_GIF_OK && at < bytes;) {
        const size_t n = bytes - at < 1024 ? bytes - at : 1024;
        if (!yield(ctx)) result = TD_GIF_CANCELLED;
        else if (fread(data + at, 1, n, file) != n) result = TD_GIF_FORMAT;
        at += (uint32_t)n;
    }
    fclose(file);
    if (result != TD_GIF_OK) {
        heap_caps_free(data);
        return result;
    }
    *out = data;
    *size = bytes;
    return TD_GIF_OK;
}

static bool close_player(gif_tile_t *tile)
{
    if (!ui_gif_player_close(tile->player)) return false;
    tile->player = NULL;
    heap_caps_free(tile->encoded);
    tile->encoded = NULL;
    return true;
}

static td_gif_result_t open_player(gif_tile_t *tile)
{
    uint8_t *data = NULL;
    uint32_t bytes = 0;
    td_gif_result_t result = read_asset(tile->id, decode_yield, tile, &data, &bytes);
    ui_gif_ops_t ops = {allocate, release, now_ms, decode_yield, in_use, present};
    uint8_t *poster = tile->poster;
    if (result == TD_GIF_OK) result = ui_gif_player_open(data, bytes, &poster, &ops, tile, &tile->player);
    if (result != TD_GIF_OK) {
        heap_caps_free(data);
        return result;
    }
    tile->encoded = data;
    const td_gif_info_t *info = ui_gif_player_info(tile->player);
    if (info->width != tile->width || info->height != tile->height) {
        close_player(tile);
        return TD_GIF_FORMAT;
    }
    return TD_GIF_OK;
}

static uint64_t run_tile(gif_tile_t *tile)
{
    portENTER_CRITICAL(&s_lock);
    tile->generation = s.generation;
    const ui_gif_visibility_t visibility = {tile->page == s.page, s.sliding, s.idle, s.dimmed, s.off};
    const bool shown = tile->image != NULL;
    portEXIT_CRITICAL(&s_lock);
    tile->yielded_us = esp_timer_get_time();
    const bool open = shown && !tile->failed && visibility.on_page && !visibility.idle && !visibility.off;
    if (!open) tile->retry_ms = 0;
    if (!open || now_ms(NULL) < tile->retry_ms) {
        if (tile->player) {
            ui_gif_player_tick(tile->player, false);
            if (!close_player(tile)) return UINT64_MAX;
        }
        return open ? tile->retry_ms : UINT64_MAX;
    }
    tile->retry_ms = 0;
    td_gif_result_t result = tile->player ? TD_GIF_OK : open_player(tile);
    if (result == TD_GIF_OK && current(tile)) result = ui_gif_player_tick(tile->player, ui_gif_should_play(visibility));
    if (result == TD_GIF_OK || result == TD_GIF_CANCELLED) {
        return tile->player ? ui_gif_player_due_ms(tile->player) : UINT64_MAX;
    }
    if (result == TD_GIF_MEMORY) tile->retry_ms = now_ms(NULL) + RETRY_MS;
    else tile->failed = true;
    if (tile->player) {
        ui_gif_player_tick(tile->player, false);
        close_player(tile);
    }
    return result == TD_GIF_MEMORY ? tile->retry_ms : UINT64_MAX;
}

static bool release_all(unsigned count)
{
    bool released = true;
    for (unsigned i = 0; i < count; i++) {
        gif_tile_t *tile = &s.tiles[i];
        if (in_use(tile, tile->poster) || !close_player(tile)) {
            released = false;
            continue;
        }
        heap_caps_free(tile->poster);
        tile->poster = NULL;
    }
    return released;
}

static void worker(void *ctx)
{
    (void)ctx;
    for (;;) {
        portENTER_CRITICAL(&s_lock);
        const bool reset = s.reset;
        const bool cleared = s.cleared;
        const unsigned count = s.count;
        portEXIT_CRITICAL(&s_lock);
        if (reset) {
            if (cleared) {
                ulTaskNotifyTake(pdTRUE, portMAX_DELAY);
            } else if (release_all(count)) {
                portENTER_CRITICAL(&s_lock);
                s.cleared = true;
                portEXIT_CRITICAL(&s_lock);
            } else {
                vTaskDelay(1);
            }
            continue;
        }
        uint64_t due = UINT64_MAX;
        for (unsigned i = 0; i < count; i++) {
            const uint64_t tile_due = run_tile(&s.tiles[i]);
            if (tile_due < due) due = tile_due;
        }
        TickType_t wait = portMAX_DELAY;
        if (due != UINT64_MAX) {
            const uint64_t now = now_ms(NULL);
            wait = due > now ? pdMS_TO_TICKS(due - now) : 0;
            if (!wait) wait = 1;
        }
        ulTaskNotifyTake(pdTRUE, wait);
    }
}

static void show(gif_tile_t *tile, const uint8_t *pixels)
{
    if (!tile->image) return;
    portENTER_CRITICAL(&s_lock);
    const uint16_t width = tile->width, height = tile->height;
    portEXIT_CRITICAL(&s_lock);
    lv_image_dsc_t *dsc = &tile->descriptors[tile->descriptor ^= 1];
    for (unsigned i = 0; i < 2; i++) {
        lv_image_cache_drop(&tile->descriptors[i]);
        lv_image_header_cache_drop(&tile->descriptors[i]);
    }
    *dsc = (lv_image_dsc_t){
        .header = {.magic = LV_IMAGE_HEADER_MAGIC, .cf = LV_COLOR_FORMAT_RGB565A8,
                   .w = width, .h = height, .stride = width * 2},
        .data_size = (uint32_t)width * height * 3,
        .data = pixels,
    };
    lv_image_set_src(tile->image, pixels ? dsc : NULL);
    lv_obj_center(tile->image);
}

static void show_posters(void)
{
    for (unsigned i = 0; i < s.count; i++) {
        gif_tile_t *tile = &s.tiles[i];
        portENTER_CRITICAL(&s_lock);
        const uint8_t *poster = tile->poster;
        const bool changed = tile->front != poster;
        portEXIT_CRITICAL(&s_lock);
        if (changed) show(tile, poster);
        portENTER_CRITICAL(&s_lock);
        tile->front = poster;
        tile->ready = NULL;
        portEXIT_CRITICAL(&s_lock);
    }
}

static void tick(lv_timer_t *timer)
{
    (void)timer;
    if (!s.ui_active) return;
    bool off = s.off, idle = s.idle;
    power_info_t power;
    if (power_get_info(&power, esp_timer_get_time()) == ESP_OK) {
        off = power.state == POWER_OFF;
        idle = power.idle_face;
    }
    portENTER_CRITICAL(&s_lock);
    const bool dimmed = s.dim_request;
    portEXIT_CRITICAL(&s_lock);
    if (s.off != off || s.idle != idle || s.dimmed != dimmed) {
        portENTER_CRITICAL(&s_lock);
        s.off = off;
        s.idle = idle;
        s.dimmed = dimmed;
        s.generation++;
        portEXIT_CRITICAL(&s_lock);
        show_posters();
        notify();
    }
    bool consumed = false;
    for (unsigned i = 0; i < s.count; i++) {
        gif_tile_t *tile = &s.tiles[i];
        portENTER_CRITICAL(&s_lock);
        const uint8_t *pixels = tile->ready;
        const uint64_t deadline = tile->deadline;
        portEXIT_CRITICAL(&s_lock);
        if (!pixels) continue;
        const bool expired = deadline && now_ms(NULL) >= deadline;
        if (!expired) show(tile, pixels);
        portENTER_CRITICAL(&s_lock);
        if (!expired) tile->front = pixels;
        tile->ready = NULL;
        portEXIT_CRITICAL(&s_lock);
        consumed = true;
    }
    if (consumed) notify();
}

void ui_gif_discard(void)
{
    for (unsigned i = 0; i < s.staged_count; i++) heap_caps_free(s.staged[i].poster);
    s.staged_count = 0;
}

static void stage(staged_tile_t *entry)
{
    int64_t yielded_us = esp_timer_get_time();
    uint8_t *data = NULL;
    uint32_t bytes = 0;
    if (read_asset(entry->id, prepare_yield, &yielded_us, &data, &bytes) != TD_GIF_OK) return;
    td_gif_t *gif = NULL;
    if (td_gif_open(data, bytes, prepare_yield, &yielded_us, &gif) == TD_GIF_OK) {
        const td_gif_info_t *info = td_gif_info(gif);
        const size_t size = (size_t)info->width * info->height * 3;
        uint8_t *poster = allocate(NULL, size);
        if (poster && td_gif_render(gif, 0, poster, size, prepare_yield, &yielded_us) == TD_GIF_OK) {
            entry->poster = poster;
            entry->width = info->width;
            entry->height = info->height;
        } else {
            heap_caps_free(poster);
        }
        td_gif_close(gif);
    }
    heap_caps_free(data);
}

void ui_gif_prepare(const td_layout_t *layout)
{
    ui_gif_discard();
#if !CONFIG_TD_GIF_CAPS
    (void)layout;
    return;
#endif
    for (unsigned p = 0; p < layout->page_count; p++) {
        unsigned page_tiles = 0;
        for (unsigned t = 0; t < layout->pages[p].tile_count; t++) {
            const ui_tile_t *tile = &layout->pages[p].tiles[t];
            if (tile->render != UI_RENDER_GIF || !tile->icon || strncmp(tile->icon, "sha256:", 7) ||
                !td_asset_id_valid(tile->icon + 7) || page_tiles >= TD_GIF_PAGE_TILES ||
                s.staged_count >= GIF_TILES) continue;
            page_tiles++;
            staged_tile_t *entry = &s.staged[s.staged_count++];
            memset(entry, 0, sizeof *entry);
            memcpy(entry->id, tile->icon + 7, sizeof entry->id);
            entry->page = (uint8_t)p;
            stage(entry);
        }
    }
}

void ui_gif_begin(void)
{
    s.ui_active = false;
    if (s.worker) {
        portENTER_CRITICAL(&s_lock);
        s.reset = true;
        s.cleared = false;
        s.generation++;
        portEXIT_CRITICAL(&s_lock);
        for (unsigned i = 0; i < s.count; i++) {
            gif_tile_t *tile = &s.tiles[i];
            if (tile->image) lv_image_set_src(tile->image, NULL);
            for (unsigned d = 0; d < 2; d++) {
                lv_image_cache_drop(&tile->descriptors[d]);
                lv_image_header_cache_drop(&tile->descriptors[d]);
            }
            portENTER_CRITICAL(&s_lock);
            tile->front = tile->ready = NULL;
            tile->image = NULL;
            portEXIT_CRITICAL(&s_lock);
        }
        notify();
        for (;;) {
            portENTER_CRITICAL(&s_lock);
            const bool cleared = s.cleared;
            portEXIT_CRITICAL(&s_lock);
            if (cleared) break;
            vTaskDelay(1);
        }
    }
    for (unsigned i = 0; i < s.staged_count; i++) {
        gif_tile_t *tile = &s.tiles[i];
        const staged_tile_t *entry = &s.staged[i];
        memset(tile, 0, sizeof *tile);
        memcpy(tile->id, entry->id, sizeof tile->id);
        tile->page = entry->page;
        tile->width = entry->width;
        tile->height = entry->height;
        tile->poster = entry->poster;
    }
    portENTER_CRITICAL(&s_lock);
    s.count = s.staged_count;
    s.page = 0;
    s.sliding = false;
    portEXIT_CRITICAL(&s_lock);
    s.staged_count = 0;
    if (s.count && !s.worker && xTaskCreatePinnedToCore(worker, "gif", 4096, NULL, 1, &s.worker, 1) != pdPASS) {
        s.worker = NULL;
        for (unsigned i = 0; i < s.count; i++) heap_caps_free(s.tiles[i].poster);
        portENTER_CRITICAL(&s_lock);
        s.count = 0;
        portEXIT_CRITICAL(&s_lock);
    }
    if (s.count && !s.timer) s.timer = lv_timer_create(tick, 20, NULL);
}

bool ui_gif_build(lv_obj_t *obj, const ui_tile_t *tile, uint8_t page)
{
    if (!tile->icon || strncmp(tile->icon, "sha256:", 7)) return false;
    for (unsigned i = 0; i < s.count; i++) {
        gif_tile_t *entry = &s.tiles[i];
        if (entry->image || !entry->poster || entry->page != page || strcmp(tile->icon + 7, entry->id)) continue;
        lv_obj_t *image = lv_image_create(obj);
        lv_obj_set_clickable(image, false);
        portENTER_CRITICAL(&s_lock);
        entry->image = image;
        portEXIT_CRITICAL(&s_lock);
        show(entry, entry->poster);
        portENTER_CRITICAL(&s_lock);
        entry->front = entry->poster;
        portEXIT_CRITICAL(&s_lock);
        return true;
    }
    return false;
}

void ui_gif_set_dimmed(bool dimmed)
{
    portENTER_CRITICAL(&s_lock);
    s.dim_request = dimmed;
    portEXIT_CRITICAL(&s_lock);
}

void ui_gif_page(uint8_t page, bool sliding)
{
    if (!s.ui_active) return;
    portENTER_CRITICAL(&s_lock);
    s.page = page;
    s.sliding = sliding;
    s.generation++;
    portEXIT_CRITICAL(&s_lock);
    show_posters();
    notify();
}

void ui_gif_resume(void)
{
    s.ui_active = s.count > 0;
    if (!s.ui_active) return;
    portENTER_CRITICAL(&s_lock);
    s.reset = false;
    s.cleared = false;
    s.generation++;
    portEXIT_CRITICAL(&s_lock);
    notify();
}
