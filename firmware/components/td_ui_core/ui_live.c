#include "ui_live.h"
#include <math.h>
#include <string.h>

static const char *const k_reasons[] = {
    NULL, "layout", "page", "tile", "stream", "type", "args", "size", "sequence", "rate", "memory",
};

const char *ui_live_reason(ui_live_result_t result)
{
    return (size_t)result < sizeof k_reasons / sizeof k_reasons[0] ? k_reasons[result] : "args";
}

size_t ui_live_streams(const ui_page_t *pages, uint8_t page_count)
{
    size_t total = 0;
    for (uint8_t p = 0; p < page_count; p++) {
        for (uint8_t t = 0; pages[p].kind == UI_PAGE_GRID && t < pages[p].tile_count; t++) {
            total += pages[p].tiles[t].stream_count;
        }
    }
    return total;
}

static void clear_value(ui_live_slot_t *slot)
{
    const ui_live_slot_t keep = { .page = slot->page, .tile = slot->tile, .stream = slot->stream };
    *slot = keep;
}

void ui_live_bind(ui_live_table_t *table, ui_live_slot_t *slots, uint16_t cap, const char *layout,
                  const ui_page_t *pages, uint8_t page_count)
{
    table->slots = slots;
    table->cap = slots != NULL ? cap : 0;
    table->count = 0;
    table->dropped = 0;
    table->pages = pages;
    table->page_count = page_count;
    const size_t len = layout != NULL ? strnlen(layout, UI_LIVE_LAYOUT_MAX) : 0;
    memcpy(table->layout, layout != NULL ? layout : "", len);
    table->layout[len] = '\0';
    for (uint8_t p = 0; p < page_count; p++) {
        for (uint8_t t = 0; pages[p].kind == UI_PAGE_GRID && t < pages[p].tile_count; t++) {
            const ui_tile_t *tile = &pages[p].tiles[t];
            for (uint8_t s = 0; s < tile->stream_count; s++) {
                if (table->count < table->cap) {
                    ui_live_slot_t *slot = &slots[table->count++];
                    *slot = (ui_live_slot_t){ .page = p, .tile = tile, .stream = &tile->streams[s] };
                } else {
                    table->dropped++;
                }
            }
        }
    }
}

void ui_live_session(ui_live_table_t *table, uint32_t now_ms)
{
    table->session_ms = now_ms;
    table->window = 0;
    table->in_window = 0;
    for (uint16_t i = 0; i < table->count; i++) {
        clear_value(&table->slots[i]);
        table->slots[i].dirty = true;
    }
}

static uint32_t fnv(uint32_t hash, const void *data, size_t len)
{
    const uint8_t *bytes = data;
    for (size_t i = 0; i < len; i++) {
        hash = (hash ^ bytes[i]) * 16777619u;
    }
    return hash;
}

static uint32_t digest(const ui_live_set_t *set)
{
    uint32_t hash = 2166136261u;
    const uint8_t type = (uint8_t)set->type;
    const uint32_t count = (uint32_t)set->count;
    hash = fnv(hash, &set->ttl_ms, sizeof set->ttl_ms);
    hash = fnv(hash, &type, sizeof type);
    hash = fnv(hash, &count, sizeof count);
    return fnv(hash, set->values, set->count * sizeof set->values[0]);
}

static bool in_range(double v)
{
    return isfinite(v) && v >= -UI_LIVE_VALUE_MAX && v <= UI_LIVE_VALUE_MAX;
}

static ui_live_result_t check_value(const ui_live_set_t *set)
{
    if (set->type != UI_STREAM_NUMBER && set->type != UI_STREAM_SERIES) {
        return UI_LIVE_TYPE;
    }
    if (set->type == UI_STREAM_NUMBER ? set->count != 1 : set->count == 0) {
        return UI_LIVE_ARGS;
    }
    if (set->count > UI_LIVE_SERIES_MAX) {
        return UI_LIVE_SIZE;
    }
    for (size_t i = 0; i < set->count; i++) {
        if (!in_range(set->values[i])) {
            return UI_LIVE_ARGS;
        }
    }
    return UI_LIVE_OK;
}

static int page_of(const ui_live_table_t *table, const char *id)
{
    for (uint8_t p = 0; p < table->page_count; p++) {
        if (strcmp(table->pages[p].id, id) == 0) {
            return p;
        }
    }
    return -1;
}

static const ui_stream_t *stream_of(const ui_tile_t *tile, const char *id)
{
    for (uint8_t s = 0; s < tile->stream_count; s++) {
        if (strcmp(tile->streams[s].id, id) == 0) {
            return &tile->streams[s];
        }
    }
    return NULL;
}

static void store(ui_live_slot_t *slot, const ui_live_set_t *set)
{
    if (set->type == UI_STREAM_SERIES) {
        memcpy(slot->samples, set->values, set->count * sizeof slot->samples[0]);
        slot->count = (uint8_t)set->count;
        slot->head = 0;
    } else if (slot->count < UI_LIVE_SERIES_MAX) {
        slot->samples[(slot->head + slot->count) % UI_LIVE_SERIES_MAX] = set->values[0];
        slot->count++;
    } else {
        slot->samples[slot->head] = set->values[0];
        slot->head = (uint8_t)((slot->head + 1) % UI_LIVE_SERIES_MAX);
    }
}

ui_live_result_t ui_live_set(ui_live_table_t *table, const ui_live_set_t *set, uint32_t now_ms, uint32_t *retry_ms)
{
    if (set->n == 0 || set->ttl_ms < UI_LIVE_TTL_MIN_MS || set->ttl_ms > UI_LIVE_TTL_MAX_MS) {
        return UI_LIVE_ARGS;
    }
    if (strcmp(set->layout, table->layout) != 0) {
        return UI_LIVE_LAYOUT;
    }
    const int page = page_of(table, set->page);
    if (page < 0) {
        return UI_LIVE_PAGE;
    }
    const ui_tile_t *tile = ui_page_find_tile(&table->pages[page], set->tile);
    if (tile == NULL) {
        return UI_LIVE_TILE;
    }
    const ui_stream_t *stream = stream_of(tile, set->stream);
    if (stream == NULL) {
        return UI_LIVE_STREAM;
    }
    if (stream->type != set->type) {
        return UI_LIVE_TYPE;
    }
    const ui_live_result_t value = check_value(set);
    if (value != UI_LIVE_OK) {
        return value;
    }
    ui_live_slot_t *slot = ui_live_find(table, tile, set->stream);
    if (slot == NULL) {
        return UI_LIVE_MEMORY;
    }
    const uint32_t hash = digest(set);
    if (set->n < slot->n || (set->n == slot->n && hash != slot->digest)) {
        return UI_LIVE_SEQUENCE;
    }
    if (set->n == slot->n) {
        return UI_LIVE_OK;
    }
    const uint32_t elapsed = now_ms - table->session_ms;
    const uint32_t window = elapsed / 1000;
    const uint8_t mine = slot->window == window ? slot->in_window : 0;
    const uint8_t all = table->window == window ? table->in_window : 0;
    if (mine >= UI_LIVE_HZ || all >= UI_LIVE_TOTAL_HZ) {
        if (retry_ms != NULL) {
            *retry_ms = 1000 - elapsed % 1000;
        }
        return UI_LIVE_RATE;
    }
    slot->window = window;
    slot->in_window = (uint8_t)(mine + 1);
    table->window = window;
    table->in_window = (uint8_t)(all + 1);
    slot->n = set->n;
    slot->digest = hash;
    slot->set = true;
    slot->stale = false;
    slot->dirty = true;
    slot->expires_ms = now_ms + set->ttl_ms;
    store(slot, set);
    return UI_LIVE_OK;
}

bool ui_live_expire(ui_live_table_t *table, uint32_t now_ms)
{
    bool changed = false;
    for (uint16_t i = 0; i < table->count; i++) {
        ui_live_slot_t *slot = &table->slots[i];
        if (slot->set && !slot->stale && (int32_t)(now_ms - slot->expires_ms) >= 0) {
            slot->stale = true;
            slot->dirty = true;
            changed = true;
        }
    }
    return changed;
}

bool ui_live_offline(ui_live_table_t *table)
{
    bool changed = false;
    for (uint16_t i = 0; i < table->count; i++) {
        ui_live_slot_t *slot = &table->slots[i];
        if (slot->set && !slot->stale) {
            slot->stale = true;
            slot->dirty = true;
            changed = true;
        }
    }
    return changed;
}

ui_live_slot_t *ui_live_find(ui_live_table_t *table, const ui_tile_t *tile, const char *stream)
{
    for (uint16_t i = 0; i < table->count; i++) {
        if (table->slots[i].tile == tile && strcmp(table->slots[i].stream->id, stream) == 0) {
            return &table->slots[i];
        }
    }
    return NULL;
}

uint8_t ui_live_samples(const ui_live_slot_t *slot, double out[UI_LIVE_SERIES_MAX])
{
    for (uint8_t i = 0; i < slot->count; i++) {
        out[i] = slot->samples[(slot->head + i) % UI_LIVE_SERIES_MAX];
    }
    return slot->count;
}
