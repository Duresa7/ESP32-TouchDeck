#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include "unity.h"
#include "key_engine.h"
#include "ui_pager.h"
#include "ui_pages.h"
#include "td_layout.h"

/* A tile with no icon and no action, as the page geometry tests need. */
#define TILE(id, label, row, col, row_span, col_span) \
    { id, label, row, col, row_span, col_span, NULL, { .type = TD_ACTION_NONE } }

typedef struct {
    unsigned key_calls;
    unsigned mod_calls;
    uint8_t last_key;
    uint8_t last_mods;
    key_mod_t last_mod;
} recorder_t;

static void record_key(void *ctx, uint8_t usage, uint8_t mods)
{
    recorder_t *rec = ctx;
    rec->key_calls++;
    rec->last_key = usage;
    rec->last_mods = mods;
}

static void record_mod(void *ctx, key_mod_t mod)
{
    recorder_t *rec = ctx;
    rec->mod_calls++;
    rec->last_mod = mod;
}

static const ui_action_ops_t k_ops = { .key = record_key, .mod = record_mod };

/* Copies of the deck's pager config in ui_deck.c and grid in ui_theme.c. */
static const ui_pager_cfg_t k_pager_cfg = { .page_w = 1024, .slop = 16, .swipe = 96 };
static const ui_rect_t k_area = { .x = 28, .y = 76, .w = 968, .h = 464 };
#define GAP 16

static const struct {
    const char *id;
    key_mod_t mod;
} k_mods[] = {
    { "shift", KEY_MOD_SHIFT },
    { "ctrl", KEY_MOD_CTRL },
    { "alt", KEY_MOD_ALT },
    { "win", KEY_MOD_WIN },
};

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

static const ui_tile_t *tile_by_id(const ui_page_t *page, const char *id)
{
    const ui_tile_t *tile = ui_page_find_tile(page, id);
    TEST_ASSERT_NOT_NULL_MESSAGE(tile, id);
    return tile;
}

TEST_CASE("every F tile on the F-Keys page sends its usage, F1 as 58 up to F12 as 69", "[fkey_page]")
{
    const ui_page_t *page = page_by_id("fkeys");
    for (unsigned n = 1; n <= 12; n++) {
        char id[8];
        snprintf(id, sizeof id, "f%u", n);
        const td_action_t action = tile_by_id(page, id)->action;
        TEST_ASSERT_EQUAL_MESSAGE(TD_ACTION_KEY, action.type, id);
        TEST_ASSERT_EQUAL_UINT8_MESSAGE(57 + n, action.key.usage, id);
        TEST_ASSERT_EQUAL_UINT8_MESSAGE(key_usage_f(n), action.key.usage, id);
        TEST_ASSERT_EQUAL_UINT8_MESSAGE(0, action.key.mods, id);
    }
}

TEST_CASE("every modifier tile on the F-Keys page toggles its own modifier", "[fkey_page]")
{
    const ui_page_t *page = page_by_id("fkeys");
    for (size_t i = 0; i < sizeof k_mods / sizeof k_mods[0]; i++) {
        const td_action_t action = tile_by_id(page, k_mods[i].id)->action;
        TEST_ASSERT_EQUAL_MESSAGE(TD_ACTION_MOD, action.type, k_mods[i].id);
        TEST_ASSERT_EQUAL_MESSAGE(k_mods[i].mod, action.mod.mod, k_mods[i].id);
    }
}

TEST_CASE("a click on a bound tile runs exactly one op with the tile's argument", "[fkey_page]")
{
    const ui_page_t *page = page_by_id("fkeys");

    recorder_t rec = { 0 };
    TEST_ASSERT_TRUE(ui_tile_click(tile_by_id(page, "f5"), &k_ops, &rec));
    TEST_ASSERT_EQUAL_UINT(1, rec.key_calls);
    TEST_ASSERT_EQUAL_UINT(0, rec.mod_calls);
    TEST_ASSERT_EQUAL_UINT8(62, rec.last_key);
    TEST_ASSERT_EQUAL_UINT8(0, rec.last_mods);

    rec = (recorder_t){ 0 };
    TEST_ASSERT_TRUE(ui_tile_click(tile_by_id(page, "shift"), &k_ops, &rec));
    TEST_ASSERT_EQUAL_UINT(0, rec.key_calls);
    TEST_ASSERT_EQUAL_UINT(1, rec.mod_calls);
    TEST_ASSERT_EQUAL(KEY_MOD_SHIFT, rec.last_mod);
}

TEST_CASE("a click calls no op for an unbound tile, a missing argument or a missing op", "[fkey_page]")
{
    const ui_tile_t unbound = TILE("unbound", "Unbound", 0, 0, 1, 1);
    const ui_tile_t *f5 = tile_by_id(page_by_id("fkeys"), "f5");
    const ui_tile_t *shift = tile_by_id(page_by_id("fkeys"), "shift");
    recorder_t rec = { 0 };

    TEST_ASSERT_EQUAL(TD_ACTION_NONE, unbound.action.type);
    TEST_ASSERT_FALSE_MESSAGE(ui_tile_click(&unbound, &k_ops, &rec), "a tile with no binding");
    TEST_ASSERT_FALSE_MESSAGE(ui_tile_click(NULL, &k_ops, &rec), "no tile");
    TEST_ASSERT_FALSE_MESSAGE(ui_tile_click(f5, NULL, &rec), "no ops");

    const ui_action_ops_t no_key = { .mod = record_mod };
    const ui_action_ops_t no_mod = { .key = record_key };
    TEST_ASSERT_FALSE_MESSAGE(ui_tile_click(f5, &no_key, &rec), "a key tile with no key op");
    TEST_ASSERT_FALSE_MESSAGE(ui_tile_click(shift, &no_mod, &rec), "a modifier tile with no modifier op");

    TEST_ASSERT_EQUAL_UINT(0, rec.key_calls);
    TEST_ASSERT_EQUAL_UINT(0, rec.mod_calls);
}

typedef struct {
    int16_t dx;
    int16_t dy;
} step_t;

typedef struct {
    const char *name;
    step_t steps[2];
    size_t step_count;
    ui_press_t press;
    unsigned calls;
} gesture_t;

static const gesture_t k_gestures[] = {
    { "left swipe past the threshold", { { -30, 0 }, { -120, 0 } }, 2, UI_PRESS_SWIPED, 0 },
    { "rightward swipe at page 0, a blocked bounce", { { 30, 0 }, { 120, 0 } }, 2, UI_PRESS_SWIPED, 0 },
    { "sideways drag short of the threshold", { { -50, 0 } }, 1, UI_PRESS_DRAG, 0 },
    { "vertical move past the slop", { { 0, 40 } }, 1, UI_PRESS_MOVED, 0 },
    { "plain tap", { { 0, 0 } }, 0, UI_PRESS_TAP, 1 },
};

/*
 * Copies the tap rule of ui_deck.c on_release, minus its LVGL checks: the deck clicks a tile only for a press that
 * ui_pager_release calls a tap. A swipe on the board is the proof of the deck itself.
 */
static unsigned calls_after(const ui_page_t *page, const ui_tile_t *tile, const gesture_t *gesture)
{
    size_t page_count = 0;
    builtin_pages(&page_count);
    ui_pager_t pager;
    ui_pager_init(&pager, &k_pager_cfg, (uint8_t)page_count);

    const ui_rect_t r = ui_tile_rect(page, tile, k_area, GAP);
    const int16_t x = (int16_t)(r.x + r.w / 2);
    const int16_t y = (int16_t)(r.y + r.h / 2);
    ui_pager_press(&pager, x, y);
    for (size_t i = 0; i < gesture->step_count; i++) {
        ui_pager_move(&pager, (int16_t)(x + gesture->steps[i].dx), (int16_t)(y + gesture->steps[i].dy));
    }
    TEST_ASSERT_EQUAL_MESSAGE(gesture->press, pager.press, gesture->name);

    recorder_t rec = { 0 };
    if (ui_pager_release(&pager)) {
        ui_tile_click(tile, &k_ops, &rec);
    }
    return rec.key_calls + rec.mod_calls;
}

TEST_CASE("a press that starts on a tile and turns into a swipe, a drag or a move calls nothing, a tap calls once",
          "[fkey_page]")
{
    const ui_page_t *page = page_by_id("fkeys");
    for (size_t t = 0; t < page->tile_count; t++) {
        for (size_t g = 0; g < sizeof k_gestures / sizeof k_gestures[0]; g++) {
            char message[64];
            snprintf(message, sizeof message, "%s: %s", page->tiles[t].id, k_gestures[g].name);
            TEST_ASSERT_EQUAL_UINT_MESSAGE(k_gestures[g].calls, calls_after(page, &page->tiles[t], &k_gestures[g]),
                                           message);
        }
    }
}

TEST_CASE("a tile does only what its own action says, whatever its id", "[fkey_page]")
{
    const ui_tile_t shift_as_key = {
        "shift", "Shift", 0, 0, 1, 1, NULL, { .type = TD_ACTION_KEY, .key = { .usage = 62, .mods = 1 | 2 } },
    };
    recorder_t rec = { 0 };
    TEST_ASSERT_TRUE(ui_tile_click(&shift_as_key, &k_ops, &rec));
    TEST_ASSERT_EQUAL_UINT(1, rec.key_calls);
    TEST_ASSERT_EQUAL_UINT_MESSAGE(0, rec.mod_calls, "a tile named shift with a key action toggles no modifier");
    TEST_ASSERT_EQUAL_UINT8(62, rec.last_key);
    TEST_ASSERT_EQUAL_UINT8(1 | 2, rec.last_mods);

    const ui_page_t *fkeys = page_by_id("fkeys");
    for (size_t t = 0; t < fkeys->tile_count; t++) {
        TEST_ASSERT_TRUE_MESSAGE(fkeys->tiles[t].action.type != TD_ACTION_NONE, fkeys->tiles[t].id);
    }
}
