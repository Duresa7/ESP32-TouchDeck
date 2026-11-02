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

typedef struct {
    char id[TD_ASSET_ID_LEN + 1];
    uint8_t page;
    lv_obj_t *image;
    lv_image_dsc_t descriptors[2];
    unsigned descriptor;
    const uint8_t *front, *ready;
    uint64_t deadline;
    uint16_t width, height;
    uint8_t *poster, *encoded;
    ui_gif_player_t *player;
    uint32_t generation;
    int64_t yielded_us;
    bool failed;
} gif_tile_t;

static struct {
    gif_tile_t tiles[TD_LAYOUT_MAX_PAGES * TD_GIF_PAGE_TILES];
    unsigned count, page;
    uint32_t generation;
    bool sliding, idle, off, dimmed, reset, cleared, preparing, ui_active;
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
    const int64_t now = esp_timer_get_time();
    if (now - tile->yielded_us >= 1000) {
        vTaskDelay(1);
        tile->yielded_us = esp_timer_get_time();
    }
    return current(tile);
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
    char path[96];
    snprintf(path, sizeof path, TD_ASSET_DIR "/%c/%s.bin", tile->id[0], tile->id);
    FILE *file = fopen(path, "rb");
    if (!file) return TD_GIF_FORMAT;
    struct stat st;
    if (fstat(fileno(file), &st) || st.st_size <= 0 || st.st_size > TD_GIF_MAX_BYTES) {
        fclose(file);
        return TD_GIF_SIZE;
    }
    const uint32_t bytes = (uint32_t)st.st_size;
    uint8_t *data = allocate(tile, bytes);
    td_gif_result_t result = data ? TD_GIF_OK : TD_GIF_MEMORY;
    for (uint32_t at = 0; result == TD_GIF_OK && at < bytes;) {
        const size_t n = bytes - at < 1024 ? bytes - at : 1024;
        if (!decode_yield(tile)) result = TD_GIF_CANCELLED;
        else if (fread(data + at, 1, n, file) != n) result = TD_GIF_FORMAT;
        at += (uint32_t)n;
    }
    fclose(file);
    ui_gif_ops_t ops = {allocate, release, now_ms, decode_yield, in_use, present};
    uint8_t *poster = tile->poster;
    if (result == TD_GIF_OK) result = ui_gif_player_open(data, bytes, &poster, &ops, tile, &tile->player);
    if (result != TD_GIF_OK) { heap_caps_free(data); return result; }
    tile->encoded = data;
    const td_gif_info_t *info = ui_gif_player_info(tile->player);
    portENTER_CRITICAL(&s_lock);
    tile->poster = poster;
    tile->width = info->width;
    tile->height = info->height;
    portEXIT_CRITICAL(&s_lock);
    return TD_GIF_OK;
}

static void worker(void *ctx)
{
    (void)ctx;
    for (;;) {
        portENTER_CRITICAL(&s_lock);
        const bool reset = s.reset;
        const bool already_cleared = s.cleared;
        const bool preparing = s.preparing;
        const unsigned count = s.count;
        portEXIT_CRITICAL(&s_lock);
        if (reset && already_cleared) {
            ulTaskNotifyTake(pdTRUE, portMAX_DELAY);
            continue;
        }
        bool cleared = true;
        if (!reset && !preparing) {
            for (unsigned i = 0; i < count; i++) {
                gif_tile_t *tile = &s.tiles[i];
                portENTER_CRITICAL(&s_lock);
                const bool inactive = tile->page != s.page;
                portEXIT_CRITICAL(&s_lock);
                if (inactive && !close_player(tile)) cleared = false;
            }
            if (!cleared) { vTaskDelay(1); continue; }
        }
        for (unsigned i = 0; i < count; i++) {
            gif_tile_t *tile = &s.tiles[i];
            portENTER_CRITICAL(&s_lock);
            tile->generation = s.generation;
            const bool active = tile->page == s.page;
            const ui_gif_visibility_t visibility = {active, s.sliding, s.idle, s.dimmed, s.off};
            portEXIT_CRITICAL(&s_lock);
            if (preparing) {
                const td_gif_result_t result = open_player(tile);
                if (result != TD_GIF_OK) tile->failed = true;
                close_player(tile);
                continue;
            }
            if (reset || !active) {
                if (reset && in_use(tile, tile->poster)) { cleared = false; continue; }
                if (!close_player(tile)) { cleared = false; continue; }
                if (reset) {
                    heap_caps_free(tile->poster);
                    tile->poster = NULL;
                    tile->failed = false;
                }
                continue;
            }
            if (tile->failed) {
                if (tile->player) {
                    ui_gif_player_tick(tile->player, false);
                    close_player(tile);
                }
                continue;
            }
            if (!tile->player && tile->poster && !ui_gif_should_play(visibility)) continue;
            td_gif_result_t result = TD_GIF_OK;
            if (!tile->player) result = open_player(tile);
            if (result == TD_GIF_OK && current(tile)) result = ui_gif_player_tick(tile->player, ui_gif_should_play(visibility));
            if (result != TD_GIF_OK && result != TD_GIF_CANCELLED) tile->failed = true;
        }
        if (preparing || (reset && cleared)) {
            portENTER_CRITICAL(&s_lock);
            s.cleared = true;
            if (preparing) {
                s.preparing = false;
                s.reset = true;
            }
            portEXIT_CRITICAL(&s_lock);
        } else {
            vTaskDelay(1);
        }
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
    power_info_t power;
    if (power_get_info(&power, esp_timer_get_time()) == ESP_OK &&
        (s.off != (power.state == POWER_OFF) || s.idle != power.idle_face)) {
        portENTER_CRITICAL(&s_lock);
        s.off = power.state == POWER_OFF;
        s.idle = power.idle_face;
        s.generation++;
        portEXIT_CRITICAL(&s_lock);
        show_posters();
    }
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
    }
}

void ui_gif_prepare(const td_layout_t *layout)
{
    unsigned count = 0;
#if CONFIG_TD_GIF_CAPS
    for (unsigned p = 0; p < layout->page_count; p++) {
        unsigned page_tiles = 0;
        for (unsigned t = 0; t < layout->pages[p].tile_count; t++) {
            const ui_tile_t *tile = &layout->pages[p].tiles[t];
            if (tile->render != UI_RENDER_GIF || !tile->icon || strncmp(tile->icon, "sha256:", 7) ||
                !td_asset_id_valid(tile->icon + 7) || page_tiles >= TD_GIF_PAGE_TILES ||
                count >= sizeof s.tiles / sizeof s.tiles[0]) continue;
            page_tiles++;
            gif_tile_t *entry = &s.tiles[count++];
            memset(entry, 0, sizeof *entry);
            memcpy(entry->id, tile->icon + 7, sizeof entry->id);
            entry->page = (uint8_t)p;
        }
    }
#else
    (void)layout;
#endif
    portENTER_CRITICAL(&s_lock);
    s.page = 0;
    s.sliding = false;
    s.count = count;
    portEXIT_CRITICAL(&s_lock);
    if (count && !s.worker) {
        if (xTaskCreatePinnedToCore(worker, "gif", 4096, NULL, 1, &s.worker, 1) != pdPASS) {
            portENTER_CRITICAL(&s_lock);
            s.count = 0;
            portEXIT_CRITICAL(&s_lock);
        }
    }
    if (!s.count) return;
    portENTER_CRITICAL(&s_lock);
    s.reset = false;
    s.cleared = false;
    s.preparing = true;
    s.generation++;
    portEXIT_CRITICAL(&s_lock);
    xTaskNotifyGive(s.worker);
    for (;;) {
        portENTER_CRITICAL(&s_lock);
        const bool preparing = s.preparing;
        portEXIT_CRITICAL(&s_lock);
        if (!preparing) return;
        vTaskDelay(1);
    }
}

void ui_gif_begin(void)
{
    if (s.count && !s.timer) s.timer = lv_timer_create(tick, 20, NULL);
}

bool ui_gif_build(lv_obj_t *obj, const ui_tile_t *tile, uint8_t page)
{
    for (unsigned i = 0; i < s.count; i++) {
        gif_tile_t *entry = &s.tiles[i];
        if (!entry->image && entry->page == page && tile->icon && strcmp(tile->icon + 7, entry->id) == 0) {
            if (!entry->poster) return false;
            entry->image = lv_image_create(obj);
            lv_obj_set_clickable(entry->image, false);
            show(entry, entry->poster);
            portENTER_CRITICAL(&s_lock);
            entry->front = entry->poster;
            portEXIT_CRITICAL(&s_lock);
            return true;
        }
    }
    return false;
}

void ui_gif_set_dimmed(bool dimmed)
{
    if (!s.ui_active || s.dimmed == dimmed) return;
    portENTER_CRITICAL(&s_lock);
    s.dimmed = dimmed;
    s.generation++;
    portEXIT_CRITICAL(&s_lock);
    show_posters();
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
    if (s.worker) xTaskNotifyGive(s.worker);
}

bool ui_gif_quiesce(void)
{
    if (!s.worker) return true;
    if (esp_lv_adapter_lock(3000) != ESP_OK) return false;
    s.ui_active = false;
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
        portEXIT_CRITICAL(&s_lock);
    }
    esp_lv_adapter_unlock();
    xTaskNotifyGive(s.worker);
    for (;;) {
        portENTER_CRITICAL(&s_lock);
        const bool cleared = s.cleared;
        portEXIT_CRITICAL(&s_lock);
        if (cleared) return true;
        vTaskDelay(1);
    }
}
