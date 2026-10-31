#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "esp_timer.h"
#include "unity.h"
#include "td_layout.h"

/* The test app embeds dev.json, full-screen.json and every file under schema/examples/invalid/, with a NUL after
 * each. */
#define INVALID_FILES(X)                                                                                              \
    X(action_missing_type) X(blank_name) X(consumer_range) X(control_character) X(duplicate_id) X(duplicate_member) \
    X(duplicate_modifier) X(empty_label) X(empty_text) X(empty_title) X(extra_property) X(full_screen_type)          \
    X(grid_bounds) X(grid_size) X(integer_with_fraction) X(invalid_id) X(invalid_utf8) X(key_range)                  \
    X(lone_surrogate) X(long_label)                                                                                   \
    X(long_name) X(long_title) X(missing_field) X(missing_page) X(orientation) X(overlap) X(oversize) X(page_count)   \
    X(page_missing_kind) X(schema_version) X(tile_count) X(tile_position) X(tile_span) X(truncated_json)             \
    X(gif_no_asset) X(gif_unknown_render) X(gif_schema_one) X(unknown_action) X(unknown_modifier) X(unknown_page_kind) X(uppercase_icon) X(wrong_type) \
    X(graph_no_stream) X(graph_text_stream) X(streams_schema_one) X(streams_without_render)

#define DECLARE(name)                                                                                                 \
    extern const char name##_start[] asm("_binary_" #name "_json_start");                                           \
    extern const char name##_end[] asm("_binary_" #name "_json_end");
INVALID_FILES(DECLARE)
DECLARE(gif)
DECLARE(graph)
DECLARE(dev)
DECLARE(full_screen)

typedef struct {
    const char *name;
    const char *start;
    const char *end;
} embedded_t;

#define ENTRY(name) { #name, name##_start, name##_end },
static const embedded_t k_invalid[] = { INVALID_FILES(ENTRY) };

static td_layout_t *parse_ok(const char *json, size_t len)
{
    td_layout_t *layout = NULL;
    td_layout_error_t err;
    const bool ok = td_layout_parse(json, len, &layout, &err);
    char message[sizeof err.key + sizeof err.message + 2];
    snprintf(message, sizeof message, "%s: %s", err.key, err.message);
    TEST_ASSERT_TRUE_MESSAGE(ok, message);
    TEST_ASSERT_NOT_NULL(layout);
    return layout;
}

static void expect_key(const char *json, const char *key)
{
    td_layout_t *layout = NULL;
    td_layout_error_t err;
    TEST_ASSERT_FALSE_MESSAGE(td_layout_parse(json, strlen(json), &layout, &err), json);
    TEST_ASSERT_NULL(layout);
    TEST_ASSERT_EQUAL_STRING_MESSAGE(key, err.key, err.message);
}

static const ui_tile_t *tile(const td_layout_t *layout, const char *id)
{
    const ui_tile_t *t = td_layout_find_tile(layout, id, NULL);
    TEST_ASSERT_NOT_NULL_MESSAGE(t, id);
    return t;
}

TEST_CASE("builtin.json parses to the milestone 1 pages, 2 pages and 22 tiles", "[td_layout]")
{
    const td_layout_t *layout = td_layout_builtin();
    TEST_ASSERT_NOT_NULL(layout);
    TEST_ASSERT_TRUE(layout->builtin);
    TEST_ASSERT_EQUAL_PTR_MESSAGE(layout, td_layout_builtin(), "the built-in layout is parsed once");
    TEST_ASSERT_EQUAL_UINT8(2, layout->page_count);
    TEST_ASSERT_EQUAL_UINT16(22, layout->tile_count);
    TEST_ASSERT_EQUAL_UINT16(0, layout->icons_wanted);
    TEST_ASSERT_EQUAL_STRING("landscape", layout->orientation);
    TEST_ASSERT_FALSE(layout->full_screen);
    TEST_ASSERT_EQUAL_STRING("fkeys", layout->pages[0].id);
    TEST_ASSERT_EQUAL_STRING("F-Keys", layout->pages[0].title);
    TEST_ASSERT_EQUAL_UINT8(16, layout->pages[0].tile_count);
    TEST_ASSERT_EQUAL_STRING("media", layout->pages[1].id);
    TEST_ASSERT_EQUAL_STRING("Media", layout->pages[1].title);
    TEST_ASSERT_EQUAL_UINT8(6, layout->pages[1].tile_count);

    for (unsigned n = 1; n <= 12; n++) {
        char id[8];
        snprintf(id, sizeof id, "f%u", n);
        const ui_tile_t *t = tile(layout, id);
        TEST_ASSERT_EQUAL_MESSAGE(TD_ACTION_KEY, t->action.type, id);
        TEST_ASSERT_EQUAL_UINT8_MESSAGE(key_usage_f(n), t->action.key.usage, id);
        TEST_ASSERT_EQUAL_UINT8_MESSAGE(0, t->action.key.mods, id);
        TEST_ASSERT_NULL_MESSAGE(t->icon, id);
    }
    static const struct {
        const char *id;
        key_mod_t mod;
    } mods[] = { { "shift", KEY_MOD_SHIFT }, { "ctrl", KEY_MOD_CTRL }, { "alt", KEY_MOD_ALT }, { "win", KEY_MOD_WIN } };
    for (size_t i = 0; i < sizeof mods / sizeof mods[0]; i++) {
        const ui_tile_t *t = tile(layout, mods[i].id);
        TEST_ASSERT_EQUAL_MESSAGE(TD_ACTION_MOD, t->action.type, mods[i].id);
        TEST_ASSERT_EQUAL_MESSAGE(mods[i].mod, t->action.mod.mod, mods[i].id);
    }
    static const struct {
        const char *id;
        uint16_t usage;
        const char *icon;
    } media[] = {
        { "prev", 182, "lv:prev" }, { "playpause", 205, "lv:play_pause" }, { "next", 181, "lv:next" },
        { "mute", 226, "lv:mute" }, { "voldown", 234, "lv:volume_mid" },  { "volup", 233, "lv:volume_max" },
    };
    for (size_t i = 0; i < sizeof media / sizeof media[0]; i++) {
        const ui_tile_t *t = tile(layout, media[i].id);
        TEST_ASSERT_EQUAL_MESSAGE(TD_ACTION_CONSUMER, t->action.type, media[i].id);
        TEST_ASSERT_EQUAL_UINT16_MESSAGE(media[i].usage, t->action.consumer.usage, media[i].id);
        TEST_ASSERT_EQUAL_STRING_MESSAGE(media[i].icon, t->icon, media[i].id);
    }
    for (uint8_t p = 0; p < layout->page_count; p++) {
        TEST_ASSERT_TRUE_MESSAGE(ui_page_valid(&layout->pages[p]), layout->pages[p].id);
    }
}

TEST_CASE("dev.json parses with every action type, its spans, its chord and its dashboard", "[td_layout]")
{
    td_layout_t *layout = parse_ok(dev_start, (size_t)(dev_end - dev_start) - 1);
    TEST_ASSERT_FALSE(layout->builtin);
    TEST_ASSERT_FALSE(layout->full_screen);
    TEST_ASSERT_EQUAL_UINT8(4, layout->page_count);
    TEST_ASSERT_EQUAL_UINT16(13, layout->tile_count);

    const ui_tile_t *chord = tile(layout, "chord");
    TEST_ASSERT_EQUAL(TD_ACTION_KEY, chord->action.type);
    TEST_ASSERT_EQUAL_UINT8(104, chord->action.key.usage);
    TEST_ASSERT_EQUAL_UINT8(1 | 2, chord->action.key.mods);
    TEST_ASSERT_EQUAL_UINT8(2, chord->col_span);
    TEST_ASSERT_EQUAL_UINT8(2, tile(layout, "launch")->row_span);

    TEST_ASSERT_EQUAL(TD_ACTION_MOD, tile(layout, "shift")->action.type);
    TEST_ASSERT_EQUAL(TD_ACTION_CONSUMER, tile(layout, "quiet")->action.type);
    TEST_ASSERT_EQUAL_STRING("lv:mute", tile(layout, "quiet")->icon);
    TEST_ASSERT_EQUAL(TD_ACTION_NONE, tile(layout, "spacer")->action.type);
    TEST_ASSERT_EQUAL(TD_ACTION_KEY, tile(layout, "f5")->action.type);
    TEST_ASSERT_EQUAL_UINT8(62, tile(layout, "f5")->action.key.usage);

    const ui_tile_t *jump = tile(layout, "accounts_jump");
    TEST_ASSERT_EQUAL(TD_ACTION_PAGE, jump->action.type);
    TEST_ASSERT_EQUAL_UINT8(td_layout_page_index(layout, "accounts"), jump->action.page.index);

    static const char *const app[][2] = { { "launch", "launch" }, { "open", "open" }, { "command", "command" } };
    for (size_t i = 0; i < 3; i++) {
        const ui_tile_t *t = tile(layout, app[i][0]);
        TEST_ASSERT_EQUAL_MESSAGE(TD_ACTION_APP, t->action.type, app[i][0]);
        TEST_ASSERT_EQUAL_STRING(app[i][1], td_action_name(&t->action));
    }
    const ui_tile_t *context = tile(layout, "five_hour");
    TEST_ASSERT_EQUAL(TD_ACTION_WIDGET, context->action.type);
    TEST_ASSERT_EQUAL_STRING("cliproxyapi", context->action.widget.source);
    TEST_ASSERT_EQUAL_STRING("five_hour_pct", context->action.widget.field);
    TEST_ASSERT_EQUAL_STRING("", context->action.widget.account);
    TEST_ASSERT_EQUAL_STRING("cliproxyapi", tile(layout, "proxy")->action.widget.source);

    const ui_page_t *accounts = &layout->pages[td_layout_page_index(layout, "accounts")];
    TEST_ASSERT_EQUAL(UI_PAGE_DASHBOARD, accounts->kind);
    TEST_ASSERT_EQUAL_STRING("cliproxyapi", accounts->source);
    TEST_ASSERT_EQUAL_UINT8(0, accounts->tile_count);
    for (uint8_t p = 0; p < layout->page_count; p++) {
        TEST_ASSERT_TRUE_MESSAGE(ui_page_valid(&layout->pages[p]), layout->pages[p].id);
    }
    td_layout_free(layout);
}

TEST_CASE("every file under schema/examples/invalid fails with the key on its first line", "[td_layout]")
{
    TEST_ASSERT_EQUAL_UINT32(46, sizeof k_invalid / sizeof k_invalid[0]);
    for (size_t i = 0; i < sizeof k_invalid / sizeof k_invalid[0]; i++) {
        const char *start = k_invalid[i].start;
        const size_t len = (size_t)(k_invalid[i].end - start) - 1;
        const char *newline = memchr(start, '\n', len);
        TEST_ASSERT_NOT_NULL_MESSAGE(newline, k_invalid[i].name);
        TEST_ASSERT_EQUAL_INT_MESSAGE(0, strncmp(start, "expected: ", 10), k_invalid[i].name);
        char expected[40];
        size_t key_len = (size_t)(newline - start) - 10;
        if (key_len > 0 && start[10 + key_len - 1] == '\r') {
            key_len--;
        }
        TEST_ASSERT_TRUE(key_len < sizeof expected);
        memcpy(expected, start + 10, key_len);
        expected[key_len] = '\0';

        td_layout_t *layout = NULL;
        td_layout_error_t err;
        const bool ok = td_layout_parse(newline + 1, len - (size_t)(newline + 1 - start), &layout, &err);
        char message[200];
        snprintf(message, sizeof message, "%s: got %s %s", k_invalid[i].name, ok ? "valid" : err.key, err.message);
        TEST_ASSERT_FALSE_MESSAGE(ok, message);
        TEST_ASSERT_NULL(layout);
        TEST_ASSERT_EQUAL_STRING_MESSAGE(expected, err.key, message);
    }
}

#define MINI_HEAD "{\"schema\":1,\"name\":\"N\",\"orientation\":\"landscape\",\"pages\":[{\"id\":\"p\",\"title\":\"P\",\"kind\":\"grid\",\"rows\":1,\"cols\":2,\"tiles\":["
#define MINI_TILE(id, col, action) "{\"id\":\"" id "\",\"label\":\"L\",\"row\":0,\"col\":" #col ",\"rowSpan\":1,\"colSpan\":1,\"icon\":null,\"action\":" action "}"
#define MINI_TAIL "]}]}"

TEST_CASE("the text rules match JSON.parse and the integer form", "[td_layout]")
{
    static const char ok[] = MINI_HEAD MINI_TILE("a", 0, "{\"type\":\"none\"}") MINI_TAIL;
    static const char spaced[] = " \t\r\n" MINI_HEAD MINI_TILE("a", 0, "{\"type\":\"none\"}") MINI_TAIL " \n";
    td_layout_free(parse_ok(ok, strlen(ok)));
    td_layout_free(parse_ok(spaced, strlen(spaced)));

    expect_key("\xEF\xBB\xBF" MINI_HEAD MINI_TILE("a", 0, "{\"type\":\"none\"}") MINI_TAIL, "file.json");
    expect_key(MINI_HEAD MINI_TILE("a", 0, "{\"type\":\"none\"}") MINI_TAIL "x", "file.json");
    expect_key(MINI_HEAD MINI_TILE("a", 0, "{\"type\":\"none\"}") ",]}]}", "file.json");
    expect_key("\v" MINI_HEAD MINI_TILE("a", 0, "{\"type\":\"none\"}") MINI_TAIL, "file.json");
    expect_key(MINI_HEAD MINI_TILE("a", 01, "{\"type\":\"none\"}") MINI_TAIL, "file.json");
    expect_key(MINI_HEAD MINI_TILE("a", -0, "{\"type\":\"none\"}") MINI_TAIL, "json.integer-form");
    expect_key(MINI_HEAD MINI_TILE("a", 1E0, "{\"type\":\"none\"}") MINI_TAIL, "json.integer-form");
    expect_key(MINI_HEAD MINI_TILE("\\u0061", 0, "{\"type\":\"none\"}") ",{\"id\":\"b\",\"label\":\"L\",\"row\":0,\"col\":1,"
               "\"rowSpan\":1,\"colSpan\":1,\"icon\":null,\"action\":{\"type\":\"none\",\"t\\u0079pe\":\"none\"}}" MINI_TAIL,
               "json.duplicate-member");
    expect_key(MINI_HEAD MINI_TILE("a", 0, "{\"type\":\"none\"}") "," MINI_TILE("\\u0061", 1, "{\"type\":\"none\"}") MINI_TAIL,
               "id.duplicate");
    expect_key(MINI_HEAD MINI_TILE("a", 0, "{\"type\":\"none\"}") "," MINI_TILE("b", 1, "{\"type\":\"page\",\"page\":\"q\"}") MINI_TAIL,
               "action.page-missing");

    char deep[64] = "";
    for (int i = 0; i < 17; i++) {
        strcat(deep, "[");
    }
    for (int i = 0; i < 17; i++) {
        strcat(deep, "]");
    }
    expect_key(deep, "file.json");
    expect_key("[[[[[[[[[[[[[[[[]]]]]]]]]]]]]]]]", "shape.type");
}

#define FULL_HEAD(value) "{\"schema\":1,\"name\":\"N\",\"orientation\":\"landscape\",\"fullScreen\":" value ",\"pages\":[{\"id\":\"p\",\"title\":\"P\",\"kind\":\"grid\",\"rows\":1,\"cols\":2,\"tiles\":["
#define NONE_TILE MINI_TILE("a", 0, "{\"type\":\"none\"}")

TEST_CASE("fullScreen is optional, false when left out, and must be a boolean", "[td_layout]")
{
    td_layout_t *layout = parse_ok(full_screen_start, (size_t)(full_screen_end - full_screen_start) - 1);
    TEST_ASSERT_TRUE(layout->full_screen);
    TEST_ASSERT_EQUAL_UINT8(2, layout->page_count);
    td_layout_free(layout);

    static const char on[] = FULL_HEAD("true") NONE_TILE MINI_TAIL;
    static const char off[] = FULL_HEAD("false") NONE_TILE MINI_TAIL;
    static const char absent[] = MINI_HEAD NONE_TILE MINI_TAIL;
    layout = parse_ok(on, strlen(on));
    TEST_ASSERT_TRUE(layout->full_screen);
    td_layout_free(layout);
    layout = parse_ok(off, strlen(off));
    TEST_ASSERT_FALSE(layout->full_screen);
    td_layout_free(layout);
    layout = parse_ok(absent, strlen(absent));
    TEST_ASSERT_FALSE(layout->full_screen);
    td_layout_free(layout);

    expect_key(FULL_HEAD("1") NONE_TILE MINI_TAIL, "shape.type");
    expect_key(FULL_HEAD("0") NONE_TILE MINI_TAIL, "shape.type");
    expect_key(FULL_HEAD("\"true\"") NONE_TILE MINI_TAIL, "shape.type");
    expect_key(FULL_HEAD("null") NONE_TILE MINI_TAIL, "shape.type");
    expect_key(FULL_HEAD("[]") NONE_TILE MINI_TAIL, "shape.type");
    expect_key(FULL_HEAD("{}") NONE_TILE MINI_TAIL, "shape.type");
}

TEST_CASE("labels count code points and blank means Unicode whitespace", "[td_layout]")
{
    static const char sixteen[] = MINI_HEAD "{\"id\":\"a\",\"label\":\"\xC3\xA9\xC3\xA9\xC3\xA9\xC3\xA9\xC3\xA9\xC3\xA9\xC3\xA9"
                                            "\xC3\xA9\xC3\xA9\xC3\xA9\xC3\xA9\xC3\xA9\xC3\xA9\xC3\xA9\xC3\xA9\xC3\xA9\",\"row\":0,"
                                            "\"col\":0,\"rowSpan\":1,\"colSpan\":1,\"icon\":null,\"action\":{\"type\":\"none\"}}"
                                            MINI_TAIL;
    td_layout_free(parse_ok(sixteen, strlen(sixteen)));
    expect_key(MINI_HEAD "{\"id\":\"a\",\"label\":\"\xE3\x80\x80\\u00a0\",\"row\":0,\"col\":0,\"rowSpan\":1,\"colSpan\":1,"
                         "\"icon\":null,\"action\":{\"type\":\"none\"}}" MINI_TAIL,
               "label.blank");
    expect_key(MINI_HEAD "{\"id\":\"a\",\"label\":\"A\\u007f\",\"row\":0,\"col\":0,\"rowSpan\":1,\"colSpan\":1,"
                         "\"icon\":null,\"action\":{\"type\":\"none\"}}" MINI_TAIL,
               "string.control-character");
}

TEST_CASE("the built-in layout outlives every replacement and only stored layouts are freed", "[td_layout]")
{
    const td_layout_t *builtin = td_layout_builtin();
    const td_layout_t *current = NULL;
    const size_t dev_len = (size_t)(dev_end - dev_start) - 1;

    TEST_ASSERT_NULL_MESSAGE(td_layout_replace(&current, builtin), "boot on the built-in layout");
    TEST_ASSERT_EQUAL_PTR(builtin, current);

    td_layout_t *stored = parse_ok(dev_start, dev_len);
    TEST_ASSERT_NULL_MESSAGE(td_layout_replace(&current, stored), "the built-in layout is never freed");
    TEST_ASSERT_EQUAL_PTR(stored, current);

    TEST_ASSERT_EQUAL_PTR_MESSAGE(stored, td_layout_replace(&current, builtin), "back to built-in frees the stored one");
    td_layout_free(stored);
    TEST_ASSERT_EQUAL_PTR(builtin, current);

    td_layout_t *again = parse_ok(dev_start, dev_len);
    TEST_ASSERT_NULL(td_layout_replace(&current, again));

    /* An invalid stored layout never reaches td_layout_replace, so the current one stays. */
    td_layout_t *bad = NULL;
    TEST_ASSERT_FALSE(td_layout_parse(k_invalid[0].start, (size_t)(k_invalid[0].end - k_invalid[0].start) - 1, &bad, NULL));
    TEST_ASSERT_NULL(bad);
    TEST_ASSERT_EQUAL_PTR(again, current);

    TEST_ASSERT_EQUAL_PTR_MESSAGE(again, td_layout_replace(&current, builtin), "recovery falls back to built-in");
    td_layout_free(again);
    td_layout_free((td_layout_t *)builtin);
    TEST_ASSERT_EQUAL_PTR_MESSAGE(builtin, td_layout_builtin(), "td_layout_free leaves the built-in layout alone");
    TEST_ASSERT_EQUAL_UINT8(2, td_layout_builtin()->page_count);
    TEST_ASSERT_NULL_MESSAGE(td_layout_replace(&current, builtin), "replacing a layout with itself frees nothing");
}

TEST_CASE("parsing builtin.json takes under 50 ms and dev.json under 200 ms", "[td_layout]")
{
    size_t len = 0;
    const char *json = td_layout_builtin_json(&len);
    const int64_t t0 = esp_timer_get_time();
    td_layout_t *layout = parse_ok(json, len);
    const int64_t t1 = esp_timer_get_time();
    td_layout_free(layout);
    layout = parse_ok(dev_start, (size_t)(dev_end - dev_start) - 1);
    const int64_t t2 = esp_timer_get_time();
    td_layout_free(layout);
    printf("td_layout: builtin %lld us, dev %lld us\n", (long long)(t1 - t0), (long long)(t2 - t1));
    TEST_ASSERT_TRUE(t1 - t0 < 50000);
    TEST_ASSERT_TRUE(t2 - t1 < 200000);
}

TEST_CASE("graph Layout fixture keeps each Tile's render and value stream", "[td_layout]")
{
    td_layout_t *layout = parse_ok(graph_start, (size_t)(graph_end - graph_start) - 1);
    const ui_tile_t *cpu = tile(layout, "cpu");
    TEST_ASSERT_EQUAL(UI_RENDER_GRAPH, cpu->render);
    TEST_ASSERT_EQUAL_UINT8(1, cpu->stream_count);
    TEST_ASSERT_EQUAL_STRING("value", cpu->streams[0].id);
    TEST_ASSERT_EQUAL(UI_STREAM_NUMBER, cpu->streams[0].type);
    TEST_ASSERT_EQUAL(TD_ACTION_WIDGET, cpu->action.type);
    TEST_ASSERT_EQUAL_STRING("cpu_pct", cpu->action.widget.field);
    const ui_tile_t *net = tile(layout, "net");
    TEST_ASSERT_EQUAL(UI_STREAM_SERIES, net->streams[0].type);
    TEST_ASSERT_EQUAL(UI_RENDER_STATIC, tile(td_layout_builtin(), td_layout_builtin()->pages[0].tiles[0].id)->render);
    td_layout_free(layout);
}

TEST_CASE("GIF Layout fixture parses with its Asset reference", "[td_layout]")
{
    td_layout_t *layout = parse_ok(gif_start, (size_t)(gif_end - gif_start) - 1);
    TEST_ASSERT_EQUAL(UI_RENDER_GIF, tile(layout, "gif")->render);
    TEST_ASSERT_EQUAL_UINT8(0, tile(layout, "gif")->stream_count);
    TEST_ASSERT_EQUAL_STRING("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", tile(layout, "gif")->icon);
    td_layout_free(layout);
}
