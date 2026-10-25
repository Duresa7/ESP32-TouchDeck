#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include "unity.h"
#include "key_engine.h"
#include "ui_pages.h"
#include "ui_tile_action.h"

typedef struct {
    unsigned key_calls;
    unsigned mod_calls;
    unsigned consumer_calls;
    uint16_t last_consumer;
} recorder_t;

static void record_key(void *ctx, uint8_t usage)
{
    (void)usage;
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

/* In grid order on the Media page. */
static const struct {
    const char *id;
    uint16_t usage;
} k_media[] = {
    { "prev", 182 }, { "playpause", 205 }, { "next", 181 }, { "mute", 226 }, { "voldown", 234 }, { "volup", 233 },
};
#define MEDIA_COUNT (sizeof k_media / sizeof k_media[0])

static const ui_page_t *page_by_id(const char *id)
{
    size_t count = 0;
    const ui_page_t *pages = ui_builtin_pages(&count);
    for (size_t i = 0; i < count; i++) {
        if (strcmp(pages[i].id, id) == 0) {
            return &pages[i];
        }
    }
    TEST_FAIL_MESSAGE(id);
    return NULL;
}

TEST_CASE("every tile on the Media page, in grid order, sends its consumer usage and no keyboard key", "[media_page]")
{
    const ui_page_t *page = page_by_id("media");
    TEST_ASSERT_EQUAL_UINT(MEDIA_COUNT, page->tile_count);
    for (size_t i = 0; i < MEDIA_COUNT; i++) {
        TEST_ASSERT_EQUAL_STRING(k_media[i].id, page->tiles[i].id);
        const ui_action_t action = ui_tile_action(&page->tiles[i]);
        TEST_ASSERT_EQUAL_MESSAGE(UI_ACTION_CONSUMER, action.kind, k_media[i].id);
        TEST_ASSERT_EQUAL_UINT16_MESSAGE(k_media[i].usage, action.consumer, k_media[i].id);
        TEST_ASSERT_EQUAL_UINT8_MESSAGE(0, action.key, k_media[i].id);
    }
}

TEST_CASE("a click on a Media tile runs the consumer op once with its usage and no key or modifier op", "[media_page]")
{
    const ui_page_t *page = page_by_id("media");
    for (size_t i = 0; i < MEDIA_COUNT; i++) {
        recorder_t rec = { 0 };
        TEST_ASSERT_TRUE_MESSAGE(ui_tile_click(&page->tiles[i], &k_ops, &rec), k_media[i].id);
        TEST_ASSERT_EQUAL_UINT_MESSAGE(1, rec.consumer_calls, k_media[i].id);
        TEST_ASSERT_EQUAL_UINT16_MESSAGE(k_media[i].usage, rec.last_consumer, k_media[i].id);
        TEST_ASSERT_EQUAL_UINT_MESSAGE(0, rec.key_calls, k_media[i].id);
        TEST_ASSERT_EQUAL_UINT_MESSAGE(0, rec.mod_calls, k_media[i].id);
    }
}

TEST_CASE("a Media click does nothing without a consumer op, and an F-Keys click never calls it", "[media_page]")
{
    const ui_page_t *media = page_by_id("media");
    const ui_action_ops_t no_consumer = { .key = record_key, .mod = record_mod };
    for (size_t i = 0; i < MEDIA_COUNT; i++) {
        recorder_t rec = { 0 };
        TEST_ASSERT_FALSE_MESSAGE(ui_tile_click(&media->tiles[i], &no_consumer, &rec), k_media[i].id);
        TEST_ASSERT_EQUAL_UINT_MESSAGE(0, rec.key_calls + rec.mod_calls + rec.consumer_calls, k_media[i].id);
    }

    const ui_page_t *fkeys = page_by_id("fkeys");
    for (size_t i = 0; i < fkeys->tile_count; i++) {
        recorder_t rec = { 0 };
        ui_tile_click(&fkeys->tiles[i], &k_ops, &rec);
        TEST_ASSERT_EQUAL_UINT_MESSAGE(0, rec.consumer_calls, fkeys->tiles[i].id);
    }
}
