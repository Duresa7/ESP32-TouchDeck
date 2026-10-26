#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include "unity.h"
#include "key_engine.h"
#include "ui_pages.h"
#include "td_layout.h"

typedef struct {
    unsigned key_calls;
    unsigned mod_calls;
    unsigned consumer_calls;
    uint16_t last_consumer;
} recorder_t;

static void record_key(void *ctx, uint8_t usage, uint8_t mods)
{
    (void)usage;
    (void)mods;
    ((recorder_t *)ctx)->key_calls++;
}

static void record_mod(void *ctx, key_mod_t mod)
{
    (void)mod;
    ((recorder_t *)ctx)->mod_calls++;
}

static void record_consumer(void *ctx, uint16_t usage)
{
    recorder_t *rec = ctx;
    rec->consumer_calls++;
    rec->last_consumer = usage;
}

static const ui_action_ops_t k_ops = { .key = record_key, .mod = record_mod, .consumer = record_consumer };

static const struct {
    const char *id;
    uint8_t row;
    uint8_t col;
    uint16_t usage;
    const char *icon;
} k_media[] = {
    { "prev", 0, 0, 182, "lv:prev" },  { "playpause", 0, 1, 205, "lv:play_pause" }, { "next", 0, 2, 181, "lv:next" },
    { "mute", 1, 0, 226, "lv:mute" },  { "voldown", 1, 1, 234, "lv:volume_mid" },  { "volup", 1, 2, 233, "lv:volume_max" },
};
#define MEDIA_COUNT (sizeof k_media / sizeof k_media[0])

/* The built-in pages, as td_layout parses them from the embedded builtin.json. */
static const ui_page_t *builtin_pages(size_t *count)
{
    const td_layout_t *layout = td_layout_builtin();
    TEST_ASSERT_NOT_NULL(layout);
    *count = layout->page_count;
    return layout->pages;
}

static const ui_page_t *page_by_id(const char *id)
{
    size_t count = 0;
    const ui_page_t *pages = builtin_pages(&count);
    for (size_t i = 0; i < count; i++) {
        if (strcmp(pages[i].id, id) == 0) {
            return &pages[i];
        }
    }
    TEST_FAIL_MESSAGE(id);
    return NULL;
}

TEST_CASE("each Media tile sits at its grid position and is bound to its consumer usage", "[media_page]")
{
    const ui_page_t *page = page_by_id("media");
    TEST_ASSERT_EQUAL_UINT(6, page->tile_count);
    for (size_t i = 0; i < MEDIA_COUNT; i++) {
        const ui_tile_t *tile = ui_page_find_tile(page, k_media[i].id);
        TEST_ASSERT_NOT_NULL_MESSAGE(tile, k_media[i].id);
        TEST_ASSERT_EQUAL_UINT8_MESSAGE(k_media[i].row, tile->row, k_media[i].id);
        TEST_ASSERT_EQUAL_UINT8_MESSAGE(k_media[i].col, tile->col, k_media[i].id);
        TEST_ASSERT_EQUAL_MESSAGE(TD_ACTION_CONSUMER, tile->action.type, k_media[i].id);
        TEST_ASSERT_EQUAL_UINT16_MESSAGE(k_media[i].usage, tile->action.consumer.usage, k_media[i].id);
        TEST_ASSERT_EQUAL_STRING_MESSAGE(k_media[i].icon, tile->icon, k_media[i].id);
    }
}

TEST_CASE("a click on a Media tile runs the consumer op once with its usage and no key or modifier op", "[media_page]")
{
    const ui_page_t *page = page_by_id("media");
    for (size_t i = 0; i < MEDIA_COUNT; i++) {
        const ui_tile_t *tile = ui_page_find_tile(page, k_media[i].id);
        recorder_t rec = { 0 };
        TEST_ASSERT_TRUE_MESSAGE(ui_tile_click(tile, &k_ops, &rec), k_media[i].id);
        TEST_ASSERT_EQUAL_UINT_MESSAGE(1, rec.consumer_calls, k_media[i].id);
        TEST_ASSERT_EQUAL_UINT16_MESSAGE(k_media[i].usage, rec.last_consumer, k_media[i].id);
        TEST_ASSERT_EQUAL_UINT_MESSAGE(0, rec.key_calls, k_media[i].id);
        TEST_ASSERT_EQUAL_UINT_MESSAGE(0, rec.mod_calls, k_media[i].id);
    }
}

TEST_CASE("a Media click does nothing without a consumer op and returns false", "[media_page]")
{
    const ui_page_t *media = page_by_id("media");
    const ui_action_ops_t no_consumer = { .key = record_key, .mod = record_mod };
    for (size_t i = 0; i < MEDIA_COUNT; i++) {
        const ui_tile_t *tile = ui_page_find_tile(media, k_media[i].id);
        recorder_t rec = { 0 };
        TEST_ASSERT_FALSE_MESSAGE(ui_tile_click(tile, &no_consumer, &rec), k_media[i].id);
        TEST_ASSERT_EQUAL_UINT_MESSAGE(0, rec.key_calls + rec.mod_calls + rec.consumer_calls, k_media[i].id);
    }
}
