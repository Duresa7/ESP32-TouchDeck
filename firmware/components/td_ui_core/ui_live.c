#include "ui_live.h"
#include <assert.h>
#include <math.h>
#include <string.h>

_Static_assert(UI_LIVE_RELEASE_MAX > UI_LIVE_IMAGES_MAX, "a drained table always has room for one more image");
_Static_assert(UI_LIVE_RELEASE_MAX <= UINT8_MAX, "released_count counts to UI_LIVE_RELEASE_MAX");

static const char *const k_reasons[] = {
    NULL, "layout", "page", "tile", "stream", "type", "args", "size", "sequence", "rate", "memory", "asset",
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

static void release(ui_live_table_t *table, const char *id)
{
    if (id[0] != '\0') {
        assert(table->released_count < UI_LIVE_RELEASE_MAX);
        memcpy(table->released[table->released_count++], id, UI_LIVE_IMAGE_ID_LEN + 1);
    }
}

static uint8_t held(const ui_live_table_t *table)
{
    uint8_t count = 0;
    for (uint16_t i = 0; i < table->count; i++) {
        const ui_live_slot_t *slot = &table->slots[i];
        count = (uint8_t)(count + (slot->stream->type == UI_STREAM_IMAGE && slot->image[0] != '\0'));
    }
    return count;
}

static void clear_value(ui_live_table_t *table, ui_live_slot_t *slot)
{
    if (slot->stream->type == UI_STREAM_IMAGE) {
        release(table, slot->image);
    }
    const ui_live_slot_t keep = { .page = slot->page, .tile = slot->tile, .stream = slot->stream };
    *slot = keep;
}

void ui_live_bind(ui_live_table_t *table, ui_live_slot_t *slots, uint16_t cap, const char *layout,
                  const ui_page_t *pages, uint8_t page_count)
{
    for (uint16_t i = 0; i < table->count; i++) {
        clear_value(table, &table->slots[i]);
    }
    table->generation++;
    table->slots = slots;
    table->cap = slots != NULL ? cap : 0;
    table->count = 0;
    table->dropped = 0;
    table->pages = pages;
    table->page_count = page_count;
    const size_t len = layout != NULL ? strnlen(layout, UI_LIVE_LAYOUT_MAX) : 0;
    memcpy(table->layout, layout != NULL ? layout : "", len);
    table->layout[len] = '\0';
    uint8_t images = 0;
    for (uint8_t p = 0; p < page_count; p++) {
        for (uint8_t t = 0; pages[p].kind == UI_PAGE_GRID && t < pages[p].tile_count; t++) {
            const ui_tile_t *tile = &pages[p].tiles[t];
            for (uint8_t s = 0; s < tile->stream_count; s++) {
                const bool image = tile->streams[s].type == UI_STREAM_IMAGE;
                if (table->count < table->cap && (!image || images < UI_LIVE_IMAGES_MAX)) {
                    images = (uint8_t)(images + image);
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
    table->generation++;
    for (uint16_t i = 0; i < table->count; i++) {
        clear_value(table, &table->slots[i]);
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
    if (set->type == UI_STREAM_TEXT) {
        return fnv(hash, set->text, strlen(set->text));
    }
    if (set->type == UI_STREAM_IMAGE) {
        const uint8_t has = set->image != NULL;
        hash = fnv(hash, &has, sizeof has);
        return has ? fnv(hash, set->image, strlen(set->image)) : hash;
    }
    return fnv(hash, set->values, set->count * sizeof set->values[0]);
}

static bool in_range(double v)
{
    return isfinite(v) && v >= -UI_LIVE_VALUE_MAX && v <= UI_LIVE_VALUE_MAX;
}

#define IMAGE_PREFIX "sha256:"
#define IMAGE_PREFIX_LEN (sizeof IMAGE_PREFIX - 1)

static bool image_id(const char *image)
{
    if (strncmp(image, IMAGE_PREFIX, IMAGE_PREFIX_LEN) != 0 ||
        strnlen(image, IMAGE_PREFIX_LEN + UI_LIVE_IMAGE_ID_LEN + 1) != IMAGE_PREFIX_LEN + UI_LIVE_IMAGE_ID_LEN) {
        return false;
    }
    for (size_t i = IMAGE_PREFIX_LEN; image[i] != '\0'; i++) {
        if (!((image[i] >= '0' && image[i] <= '9') || (image[i] >= 'a' && image[i] <= 'f'))) {
            return false;
        }
    }
    return true;
}

static ui_live_result_t check_value(const ui_tile_t *tile, const ui_stream_t *stream, const ui_live_set_t *set)
{
    if (set->type == UI_STREAM_TEXT) {
        if (set->text == NULL) {
            return UI_LIVE_ARGS;
        }
        return strnlen(set->text, UI_LIVE_TEXT_MAX + 1) > UI_LIVE_TEXT_MAX ? UI_LIVE_SIZE : UI_LIVE_OK;
    }
    if (set->type == UI_STREAM_IMAGE) {
        return set->image == NULL || image_id(set->image) ? UI_LIVE_OK : UI_LIVE_ARGS;
    }
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
    if (tile->render == UI_RENDER_BAR && strcmp(stream->id, "value") == 0 &&
        (set->values[0] < 0 || set->values[0] > 1)) {
        return UI_LIVE_ARGS;
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

static const char *image_of(const ui_live_set_t *set)
{
    return set->image != NULL ? set->image + IMAGE_PREFIX_LEN : "";
}

static void store(ui_live_table_t *table, ui_live_slot_t *slot, const ui_live_set_t *set)
{
    if (set->type == UI_STREAM_TEXT) {
        const size_t len = strlen(set->text);
        memcpy(slot->text, set->text, len);
        slot->text[len] = '\0';
    } else if (set->type == UI_STREAM_IMAGE) {
        const char *id = image_of(set);
        if (strcmp(slot->image, id) != 0) {
            release(table, slot->image);
            const size_t len = strlen(id);
            memcpy(slot->image, id, len);
            slot->image[len] = '\0';
        }
    } else if (set->type == UI_STREAM_SERIES) {
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

static ui_live_slot_t *find(const ui_live_table_t *table, const ui_tile_t *tile, const char *stream)
{
    for (uint16_t i = 0; i < table->count; i++) {
        if (table->slots[i].tile == tile && strcmp(table->slots[i].stream->id, stream) == 0) {
            return &table->slots[i];
        }
    }
    return NULL;
}

static ui_live_result_t prepare(const ui_live_table_t *table, const ui_live_set_t *set, uint32_t now_ms,
                                uint32_t *retry_ms, ui_live_slot_t **changed, uint32_t *hash)
{
    *changed = NULL;
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
    const ui_live_result_t value = check_value(tile, stream, set);
    if (value != UI_LIVE_OK) {
        return value;
    }
    ui_live_slot_t *slot = find(table, tile, set->stream);
    if (slot == NULL) {
        return UI_LIVE_MEMORY;
    }
    *hash = digest(set);
    if (set->n < slot->n || (set->n == slot->n && *hash != slot->digest)) {
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
    if (set->type == UI_STREAM_IMAGE && set->image != NULL && strcmp(slot->image, image_of(set)) != 0 &&
        table->released_count + held(table) >= UI_LIVE_RELEASE_MAX) {
        return UI_LIVE_MEMORY;
    }
    *changed = slot;
    return UI_LIVE_OK;
}

static void commit(ui_live_table_t *table, ui_live_slot_t *slot, const ui_live_set_t *set, uint32_t hash,
                   uint32_t now_ms)
{
    const uint32_t window = (now_ms - table->session_ms) / 1000;
    slot->in_window = (uint8_t)((slot->window == window ? slot->in_window : 0) + 1);
    slot->window = window;
    table->in_window = (uint8_t)((table->window == window ? table->in_window : 0) + 1);
    table->window = window;
    slot->n = set->n;
    slot->digest = hash;
    slot->set = true;
    slot->stale = false;
    slot->dirty = true;
    slot->expires_ms = now_ms + set->ttl_ms;
    store(table, slot, set);
}

ui_live_result_t ui_live_set(ui_live_table_t *table, const ui_live_set_t *set, uint32_t now_ms, uint32_t *retry_ms)
{
    ui_live_slot_t *slot = NULL;
    uint32_t hash = 0;
    const ui_live_result_t result = prepare(table, set, now_ms, retry_ms, &slot, &hash);
    if (result == UI_LIVE_OK && slot != NULL) {
        commit(table, slot, set, hash, now_ms);
    }
    return result;
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
    return find(table, tile, stream);
}

uint8_t ui_live_samples(const ui_live_slot_t *slot, double out[UI_LIVE_SERIES_MAX])
{
    for (uint8_t i = 0; i < slot->count; i++) {
        out[i] = slot->samples[(slot->head + i) % UI_LIVE_SERIES_MAX];
    }
    return slot->count;
}

ui_live_result_t ui_live_check(const ui_live_table_t *table, const ui_live_set_t *set, uint32_t now_ms,
                               uint32_t *retry_ms, bool *new_image)
{
    ui_live_slot_t *slot = NULL;
    uint32_t hash = 0;
    const ui_live_result_t result = prepare(table, set, now_ms, retry_ms, &slot, &hash);
    *new_image = result == UI_LIVE_OK && slot != NULL && set->type == UI_STREAM_IMAGE && set->image != NULL &&
                 strcmp(slot->image, image_of(set)) != 0;
    return result;
}

void ui_live_release(ui_live_table_t *table, const ui_live_ops_t *ops)
{
    char ids[UI_LIVE_RELEASE_MAX][UI_LIVE_IMAGE_ID_LEN + 1];
    ops->lock(ops->ctx);
    const uint8_t count = table->released_count;
    memcpy(ids, table->released, count * sizeof ids[0]);
    table->released_count = 0;
    ops->unlock(ops->ctx);
    for (uint8_t i = 0; i < count; i++) {
        ops->unpin(ops->ctx, ids[i]);
    }
}

ui_live_result_t ui_live_apply(ui_live_table_t *table, const ui_live_set_t *set, uint32_t now_ms, uint32_t *retry_ms,
                               const ui_live_ops_t *ops)
{
    bool new_image = false;
    uint32_t generation = 0;
    if (set->type == UI_STREAM_IMAGE) {
        ops->lock(ops->ctx);
        const ui_live_result_t checked = ui_live_check(table, set, now_ms, retry_ms, &new_image);
        generation = table->generation;
        ops->unlock(ops->ctx);
        if (checked != UI_LIVE_OK) {
            return checked;
        }
    }
    void *pixels = NULL;
    if (new_image) {
        const char *id = image_of(set);
        const ui_live_result_t pinned = ops->pin(ops->ctx, id);
        if (pinned != UI_LIVE_OK) {
            return pinned;
        }
        const ui_live_result_t decoded = ops->decode(ops->ctx, id, &pixels);
        if (decoded != UI_LIVE_OK) {
            ops->unpin(ops->ctx, id);
            return decoded;
        }
    }
    void *displaced = NULL;
    bool shown = false;
    ui_live_result_t result = UI_LIVE_LAYOUT;
    ops->lock(ops->ctx);
    if (set->type != UI_STREAM_IMAGE || table->generation == generation) {
        ui_live_slot_t *slot = NULL;
        uint32_t hash = 0;
        result = prepare(table, set, now_ms, retry_ms, &slot, &hash);
        if (result == UI_LIVE_OK && slot != NULL) {
            const bool changed = set->type == UI_STREAM_IMAGE && strcmp(slot->image, image_of(set)) != 0;
            commit(table, slot, set, hash, now_ms);
            if (changed) {
                displaced = ops->show(ops->ctx, (uint16_t)(slot - table->slots), pixels);
                shown = true;
            }
        }
    }
    ops->unlock(ops->ctx);
    if (new_image && !shown) {
        ops->discard(ops->ctx, pixels);
        ops->unpin(ops->ctx, image_of(set));
    }
    if (displaced != NULL) {
        ops->discard(ops->ctx, displaced);
    }
    ui_live_release(table, ops);
    return result;
}
