#include "ui_priv.h"
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include "esp_heap_caps.h"
#include "esp_log.h"

static const char *TAG = "ui";

/* schema/icon-format.md: magic, width, height, format and a reserved word, then RGB565 and A8 planes. */
#define ICON_SIDE 64
#define ICON_HEADER 12
#define ICON_PIXELS (ICON_SIDE * ICON_SIDE * 3)
#define ICON_FILE (ICON_HEADER + ICON_PIXELS)
#define ICON_FORMAT_RGB565A8 1
#define ICON_CACHE_MAX 128
#define ICON_ID_LEN 64

typedef struct {
    char id[ICON_ID_LEN + 1];
    bool keep;
    lv_image_dsc_t dsc;
    uint8_t pixels[ICON_PIXELS];
} icon_t;

static icon_t *s_cache[ICON_CACHE_MAX];

static const struct {
    const char *name;
    const char *symbol;
} k_symbols[] = {
    { "lv:prev", LV_SYMBOL_PREV },
    { "lv:play_pause", LV_SYMBOL_PLAY LV_SYMBOL_PAUSE },
    { "lv:next", LV_SYMBOL_NEXT },
    { "lv:mute", LV_SYMBOL_MUTE },
    { "lv:volume_mid", LV_SYMBOL_VOLUME_MID },
    { "lv:volume_max", LV_SYMBOL_VOLUME_MAX },
};

const char *ui_icon_symbol(const char *icon)
{
    for (size_t i = 0; icon != NULL && i < sizeof k_symbols / sizeof k_symbols[0]; i++) {
        if (strcmp(icon, k_symbols[i].name) == 0) {
            return k_symbols[i].symbol;
        }
    }
    return NULL;
}

/* The 16 hex digits of a `file:` icon, or NULL. */
static const char *file_id(const char *icon)
{
    if (icon != NULL && strncmp(icon, "sha256:", 7) == 0) return icon + 7;
    return icon != NULL && strncmp(icon, "file:", 5) == 0 ? icon + 5 : NULL;
}

static icon_t *find(const char *id)
{
    for (size_t i = 0; i < ICON_CACHE_MAX; i++) {
        if (s_cache[i] != NULL && strcmp(s_cache[i]->id, id) == 0) {
            return s_cache[i];
        }
    }
    return NULL;
}

const lv_image_dsc_t *ui_icon_image(const char *icon)
{
    const char *id = file_id(icon);
    const icon_t *entry = id != NULL ? find(id) : NULL;
    return entry != NULL ? &entry->dsc : NULL;
}

static uint16_t le16(const uint8_t *p)
{
    return (uint16_t)(p[0] | (p[1] << 8));
}

static icon_t *load(const char *id)
{
    char path[96];
    if (strlen(id) == 64) {
        snprintf(path, sizeof path, "/storage/assets/%c/%s.bin", id[0], id);
    } else {
        snprintf(path, sizeof path, "/storage/icons/%s.bin", id);
    }
    struct stat st;
    if (stat(path, &st) != 0) {
        return NULL;
    }
    if (st.st_size != ICON_FILE) {
        ESP_LOGW(TAG, "icon %s is %ld bytes, not %d", id, (long)st.st_size, ICON_FILE);
        return NULL;
    }
    icon_t *entry = heap_caps_calloc(1, sizeof *entry, MALLOC_CAP_SPIRAM);
    FILE *f = entry != NULL ? fopen(path, "rb") : NULL;
    if (f == NULL) {
        heap_caps_free(entry);
        return NULL;
    }
    uint8_t header[ICON_HEADER];
    const bool read = fread(header, 1, sizeof header, f) == sizeof header &&
                      fread(entry->pixels, 1, ICON_PIXELS, f) == ICON_PIXELS;
    fclose(f);
    if (!read || memcmp(header, "TDI1", 4) != 0 || le16(header + 4) != ICON_SIDE || le16(header + 6) != ICON_SIDE ||
        le16(header + 8) != ICON_FORMAT_RGB565A8 || le16(header + 10) != 0) {
        ESP_LOGW(TAG, "icon %s has a bad header", id);
        heap_caps_free(entry);
        return NULL;
    }
    strlcpy(entry->id, id, sizeof entry->id);
    entry->dsc.header.magic = LV_IMAGE_HEADER_MAGIC;
    entry->dsc.header.cf = LV_COLOR_FORMAT_RGB565A8;
    entry->dsc.header.w = ICON_SIDE;
    entry->dsc.header.h = ICON_SIDE;
    entry->dsc.header.stride = ICON_SIDE * 2;
    entry->dsc.data_size = ICON_PIXELS;
    entry->dsc.data = entry->pixels;
    return entry;
}

static void mark(const td_layout_t *layout)
{
    for (uint8_t p = 0; layout != NULL && p < layout->page_count; p++) {
        for (uint8_t t = 0; t < layout->pages[p].tile_count; t++) {
            const char *id = file_id(layout->pages[p].tiles[t].icon);
            icon_t *entry = id != NULL ? find(id) : NULL;
            if (entry != NULL) {
                entry->keep = true;
            }
        }
    }
}

/* A free slot, or the slot of an icon nobody keeps, freed. -1 when every slot holds a kept icon. */
static int free_slot(void)
{
    for (int i = 0; i < ICON_CACHE_MAX; i++) {
        if (s_cache[i] == NULL) {
            return i;
        }
    }
    for (int i = 0; i < ICON_CACHE_MAX; i++) {
        if (!s_cache[i]->keep) {
            heap_caps_free(s_cache[i]);
            s_cache[i] = NULL;
            return i;
        }
    }
    return -1;
}

ui_icon_counts_t ui_icon_prepare(const td_layout_t *layout, const td_layout_t *keep)
{
    ui_icon_counts_t counts = { 0 };
    for (size_t i = 0; i < ICON_CACHE_MAX; i++) {
        if (s_cache[i] != NULL) {
            s_cache[i]->keep = false;
        }
    }
    mark(keep);
    mark(layout);
    for (uint8_t p = 0; layout != NULL && p < layout->page_count; p++) {
        for (uint8_t t = 0; t < layout->pages[p].tile_count; t++) {
            const char *id = file_id(layout->pages[p].tiles[t].icon);
            if (id == NULL) {
                continue;
            }
            if (find(id) != NULL) {
                counts.loaded++;
                continue;
            }
            icon_t *entry = load(id);
            const int slot = entry != NULL ? free_slot() : -1;
            if (slot < 0) {
                heap_caps_free(entry);
                counts.missing++;
                continue;
            }
            entry->keep = true;
            s_cache[slot] = entry;
            counts.loaded++;
        }
    }
    return counts;
}
