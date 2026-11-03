#include <stdio.h>
#include <string.h>
#include "cJSON.h"
#include "unity.h"
#include "ui_live.h"

/* schema/live/text-vectors.json, embedded by the test app with a NUL after it. */
extern const char text_vectors_start[] asm("_binary_text_vectors_json_start");

static const ui_stream_t k_np_streams[] = {
    { "title", UI_STREAM_TEXT },
    { "artist", UI_STREAM_TEXT },
    { "cover", UI_STREAM_IMAGE },
    { "state", UI_STREAM_NUMBER },
};
static const ui_stream_t k_bar_streams[] = { { "value", UI_STREAM_NUMBER }, { "muted", UI_STREAM_NUMBER } };
static const ui_stream_t k_cover_only[] = { { "cover", UI_STREAM_IMAGE } };
static const ui_tile_t k_tiles[] = {
    { .id = "np", .label = "Now playing", .row = 0, .col = 0, .row_span = 1, .col_span = 1,
      .render = UI_RENDER_NOW_PLAYING, .stream_count = 4, .streams = k_np_streams },
    { .id = "vol", .label = "Volume", .row = 0, .col = 1, .row_span = 1, .col_span = 1, .render = UI_RENDER_BAR,
      .input = true, .stream_count = 2, .streams = k_bar_streams },
    { .id = "np2", .label = "Other", .row = 1, .col = 0, .row_span = 1, .col_span = 1,
      .render = UI_RENDER_NOW_PLAYING, .stream_count = 4, .streams = k_np_streams },
};
static const ui_page_t k_pages[] = {
    { .id = "media", .title = "Media", .rows = 2, .cols = 2, .tile_count = 3, .tiles = k_tiles },
};

#define ID_A "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
#define ID_B "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"

static ui_live_slot_t s_slots[16];
static ui_live_table_t s_table;

static void bind(void)
{
    memset(&s_table, 0, sizeof s_table);
    ui_live_bind(&s_table, s_slots, 16, "builtin", k_pages, 1);
    ui_live_session(&s_table, 1000);
    s_table.released_count = 0;
}

static ui_live_set_t text_set(const char *tile, const char *stream, uint32_t n, const char *text)
{
    return (ui_live_set_t){ .layout = "builtin", .page = "media", .tile = tile, .stream = stream, .n = n,
                            .ttl_ms = 5000, .type = UI_STREAM_TEXT, .text = text };
}

static ui_live_set_t image_set(const char *tile, uint32_t n, const char *image)
{
    return (ui_live_set_t){ .layout = "builtin", .page = "media", .tile = tile, .stream = "cover", .n = n,
                            .ttl_ms = 5000, .type = UI_STREAM_IMAGE, .image = image };
}

static ui_live_slot_t *slot_of(int tile, const char *stream)
{
    return ui_live_find(&s_table, &k_tiles[tile], stream);
}

TEST_CASE("live: a text value is kept byte for byte, empty included", "[ui_live]")
{
    bind();
    ui_live_set_t set = text_set("np", "title", 1, "Beyonc\xc3\xa9 \"live\"");
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, 1000, NULL));
    TEST_ASSERT_EQUAL_STRING("Beyonc\xc3\xa9 \"live\"", slot_of(0, "title")->text);
    TEST_ASSERT_TRUE(slot_of(0, "title")->set);
    set = text_set("np", "title", 2, "");
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, 1600, NULL));
    TEST_ASSERT_EQUAL_STRING("", slot_of(0, "title")->text);
}

TEST_CASE("live: text over UI_LIVE_TEXT_MAX bytes is refused with size and changes nothing", "[ui_live]")
{
    bind();
    char text[UI_LIVE_TEXT_MAX + 2];
    memset(text, 'x', UI_LIVE_TEXT_MAX);
    text[UI_LIVE_TEXT_MAX] = '\0';
    ui_live_set_t set = text_set("np", "artist", 1, text);
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, 1000, NULL));
    TEST_ASSERT_EQUAL_size_t(UI_LIVE_TEXT_MAX, strlen(slot_of(0, "artist")->text));
    text[UI_LIVE_TEXT_MAX] = 'y';
    text[UI_LIVE_TEXT_MAX + 1] = '\0';
    set = text_set("np", "artist", 2, text);
    TEST_ASSERT_EQUAL(UI_LIVE_SIZE, ui_live_set(&s_table, &set, 1600, NULL));
    TEST_ASSERT_EQUAL_UINT32(1, slot_of(0, "artist")->n);
    set.text = NULL;
    TEST_ASSERT_EQUAL(UI_LIVE_ARGS, ui_live_set(&s_table, &set, 1600, NULL));
}

TEST_CASE("live: every text the Companion's fit_text makes is drawn as sent, and longer text is refused",
          "[ui_live]")
{
    cJSON *root = cJSON_Parse(text_vectors_start);
    TEST_ASSERT_NOT_NULL(root);
    const cJSON *c = NULL;
    uint32_t n = 0;
    int cases = 0;
    cJSON_ArrayForEach(c, cJSON_GetObjectItemCaseSensitive(root, "cases"))
    {
        const char *name = cJSON_GetObjectItemCaseSensitive(c, "name")->valuestring;
        const char *input = cJSON_GetObjectItemCaseSensitive(c, "input")->valuestring;
        const char *output = cJSON_GetObjectItemCaseSensitive(c, "output")->valuestring;
        bind();
        ui_live_set_t set = text_set("np", "title", ++n, output);
        TEST_ASSERT_EQUAL_MESSAGE(UI_LIVE_OK, ui_live_set(&s_table, &set, 1000, NULL), name);
        TEST_ASSERT_EQUAL_STRING_MESSAGE(output, slot_of(0, "title")->text, name);
        set = text_set("np", "title", ++n, input);
        const ui_live_result_t raw = ui_live_set(&s_table, &set, 1500, NULL);
        TEST_ASSERT_EQUAL_MESSAGE(strlen(input) > UI_LIVE_TEXT_MAX ? UI_LIVE_SIZE : UI_LIVE_OK, raw, name);
        cases++;
    }
    TEST_ASSERT_TRUE(cases >= 30);
    cJSON_Delete(root);
}

TEST_CASE("live: an image is sha256 and 64 lowercase hex digits, or null to clear", "[ui_live]")
{
    bind();
    ui_live_set_t set = image_set("np", 1, "sha256:" ID_A);
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, 1000, NULL));
    TEST_ASSERT_EQUAL_STRING(ID_A, slot_of(0, "cover")->image);
    const char *bad[] = { "sha256:" "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "sha256:abc",
                          "md5:" ID_A, ID_A, "sha256:" ID_A "a", "" };
    for (size_t i = 0; i < sizeof bad / sizeof bad[0]; i++) {
        set = image_set("np", 2, bad[i]);
        TEST_ASSERT_EQUAL_MESSAGE(UI_LIVE_ARGS, ui_live_set(&s_table, &set, 1600, NULL), bad[i]);
    }
    TEST_ASSERT_EQUAL_STRING(ID_A, slot_of(0, "cover")->image);
    set = image_set("np", 2, NULL);
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, 1600, NULL));
    TEST_ASSERT_EQUAL_STRING("", slot_of(0, "cover")->image);
    TEST_ASSERT_TRUE(slot_of(0, "cover")->set);
    TEST_ASSERT_EQUAL_STRING("asset", ui_live_reason(UI_LIVE_ASSET));
}

TEST_CASE("live: a bar value is 0 to 1, and muted takes any number", "[ui_live]")
{
    bind();
    double v = 1.01;
    ui_live_set_t set = { .layout = "builtin", .page = "media", .tile = "vol", .stream = "value", .n = 1,
                          .ttl_ms = 5000, .type = UI_STREAM_NUMBER, .values = &v, .count = 1 };
    TEST_ASSERT_EQUAL(UI_LIVE_ARGS, ui_live_set(&s_table, &set, 1000, NULL));
    v = -0.01;
    TEST_ASSERT_EQUAL(UI_LIVE_ARGS, ui_live_set(&s_table, &set, 1000, NULL));
    v = 1;
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, 1000, NULL));
    set.stream = "muted";
    v = 7;
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, 1000, NULL));
}

TEST_CASE("live: an identical retry of text or an image changes nothing, a different value reusing n is refused",
          "[ui_live]")
{
    bind();
    ui_live_set_t set = text_set("np", "title", 3, "One");
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, 1000, NULL));
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, 1100, NULL));
    set.text = "Two";
    TEST_ASSERT_EQUAL(UI_LIVE_SEQUENCE, ui_live_set(&s_table, &set, 1200, NULL));
    set = image_set("np", 3, "sha256:" ID_A);
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, 1300, NULL));
    set.image = "sha256:" ID_B;
    TEST_ASSERT_EQUAL(UI_LIVE_SEQUENCE, ui_live_set(&s_table, &set, 1300, NULL));
    set.image = NULL;
    TEST_ASSERT_EQUAL(UI_LIVE_SEQUENCE, ui_live_set(&s_table, &set, 1300, NULL));
    TEST_ASSERT_EQUAL_STRING(ID_A, slot_of(0, "cover")->image);
}

TEST_CASE("live: a replaced or cleared image, a session and a new Layout release what the slots held",
          "[ui_live]")
{
    bind();
    ui_live_set_t set = image_set("np", 1, "sha256:" ID_A);
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, 1000, NULL));
    TEST_ASSERT_EQUAL_UINT8(0, s_table.released_count);
    set = image_set("np", 2, "sha256:" ID_A);
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, 1600, NULL));
    TEST_ASSERT_EQUAL_UINT8(0, s_table.released_count);
    set = image_set("np", 3, "sha256:" ID_B);
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, 2100, NULL));
    TEST_ASSERT_EQUAL_UINT8(1, s_table.released_count);
    TEST_ASSERT_EQUAL_STRING(ID_A, s_table.released[0]);
    set = image_set("np", 4, NULL);
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, 2700, NULL));
    TEST_ASSERT_EQUAL_UINT8(2, s_table.released_count);
    TEST_ASSERT_EQUAL_STRING(ID_B, s_table.released[1]);

    s_table.released_count = 0;
    set = image_set("np", 5, "sha256:" ID_A);
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, 3200, NULL));
    set = image_set("np2", 1, "sha256:" ID_B);
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, 3200, NULL));
    const uint32_t generation = s_table.generation;
    ui_live_session(&s_table, 4000);
    TEST_ASSERT_NOT_EQUAL(generation, s_table.generation);
    TEST_ASSERT_EQUAL_UINT8(2, s_table.released_count);
    TEST_ASSERT_EQUAL_STRING("", slot_of(0, "cover")->image);

    s_table.released_count = 0;
    set = image_set("np", 1, "sha256:" ID_B);
    TEST_ASSERT_EQUAL(UI_LIVE_OK, ui_live_set(&s_table, &set, 4000, NULL));
    ui_live_bind(&s_table, s_slots, 16, "other", k_pages, 1);
    TEST_ASSERT_EQUAL_UINT8(1, s_table.released_count);
    TEST_ASSERT_EQUAL_STRING(ID_B, s_table.released[0]);
}

TEST_CASE("live: image streams past UI_LIVE_IMAGES_MAX get no slot", "[ui_live]")
{
    static const ui_tile_t many[] = {
        { .id = "c1", .label = "1", .row = 0, .col = 0, .row_span = 1, .col_span = 1, .render = UI_RENDER_NOW_PLAYING,
          .stream_count = 1, .streams = k_cover_only },
        { .id = "c2", .label = "2", .row = 0, .col = 1, .row_span = 1, .col_span = 1, .render = UI_RENDER_NOW_PLAYING,
          .stream_count = 1, .streams = k_cover_only },
        { .id = "c3", .label = "3", .row = 0, .col = 2, .row_span = 1, .col_span = 1, .render = UI_RENDER_NOW_PLAYING,
          .stream_count = 1, .streams = k_cover_only },
        { .id = "c4", .label = "4", .row = 0, .col = 3, .row_span = 1, .col_span = 1, .render = UI_RENDER_NOW_PLAYING,
          .stream_count = 1, .streams = k_cover_only },
        { .id = "c5", .label = "5", .row = 0, .col = 4, .row_span = 1, .col_span = 1, .render = UI_RENDER_NOW_PLAYING,
          .stream_count = 1, .streams = k_cover_only },
    };
    static const ui_page_t page = { .id = "media", .title = "Media", .rows = 1, .cols = 5, .tile_count = 5,
                                    .tiles = many };
    memset(&s_table, 0, sizeof s_table);
    ui_live_bind(&s_table, s_slots, 16, "builtin", &page, 1);
    TEST_ASSERT_EQUAL_UINT16(UI_LIVE_IMAGES_MAX, s_table.count);
    TEST_ASSERT_EQUAL_UINT16(1, s_table.dropped);
    ui_live_set_t set = image_set("c5", 1, "sha256:" ID_A);
    TEST_ASSERT_EQUAL(UI_LIVE_MEMORY, ui_live_set(&s_table, &set, 1000, NULL));
}

#define TRACE_MAX 512
static struct {
    bool locked;
    bool decoded_locked;
    char trace[TRACE_MAX];
    int pins;
    ui_live_result_t pin_result;
    ui_live_result_t decode_result;
    int decodes;
    int shows;
    int discards;
    void *shown[16];
    int pixels_live;
    void (*during_decode)(void);
} fake;

static void note(const char *step)
{
    strncat(fake.trace, step, TRACE_MAX - strlen(fake.trace) - 1);
    strncat(fake.trace, " ", TRACE_MAX - strlen(fake.trace) - 1);
}

static void fake_lock(void *ctx)
{
    (void)ctx;
    TEST_ASSERT_FALSE(fake.locked);
    fake.locked = true;
    note("lock");
}

static void fake_unlock(void *ctx)
{
    (void)ctx;
    TEST_ASSERT_TRUE(fake.locked);
    fake.locked = false;
    note("unlock");
}

static ui_live_result_t fake_pin(void *ctx, const char *id)
{
    (void)ctx;
    TEST_ASSERT_FALSE(fake.locked);
    TEST_ASSERT_EQUAL_size_t(UI_LIVE_IMAGE_ID_LEN, strlen(id));
    note("pin");
    if (fake.pin_result == UI_LIVE_OK) {
        fake.pins++;
    }
    return fake.pin_result;
}

static void fake_unpin(void *ctx, const char *id)
{
    (void)ctx;
    TEST_ASSERT_FALSE(fake.locked);
    TEST_ASSERT_EQUAL_size_t(UI_LIVE_IMAGE_ID_LEN, strlen(id));
    note("unpin");
    fake.pins--;
}

static int s_pixel_store[64];

static ui_live_result_t fake_decode(void *ctx, const char *id, void **pixels)
{
    (void)ctx;
    (void)id;
    fake.decoded_locked = fake.locked;
    note("decode");
    if (fake.during_decode != NULL) {
        fake.during_decode();
    }
    if (fake.decode_result != UI_LIVE_OK) {
        return fake.decode_result;
    }
    *pixels = &s_pixel_store[fake.decodes++ % 64];
    fake.pixels_live++;
    return UI_LIVE_OK;
}

static void *fake_show(void *ctx, uint16_t index, void *pixels)
{
    (void)ctx;
    TEST_ASSERT_TRUE(fake.locked);
    note(pixels != NULL ? "show" : "clear");
    fake.shows++;
    void *old = fake.shown[index];
    fake.shown[index] = pixels;
    return old;
}

static void fake_discard(void *ctx, void *pixels)
{
    (void)ctx;
    TEST_ASSERT_FALSE(fake.locked);
    TEST_ASSERT_NOT_NULL(pixels);
    note("discard");
    fake.discards++;
    fake.pixels_live--;
}

static const ui_live_ops_t k_ops = {
    .lock = fake_lock, .unlock = fake_unlock, .pin = fake_pin, .unpin = fake_unpin, .decode = fake_decode,
    .show = fake_show, .discard = fake_discard,
};

static void fresh(void)
{
    memset(&fake, 0, sizeof fake);
    bind();
}

static ui_live_result_t apply(ui_live_set_t set, uint32_t now_ms)
{
    fake.trace[0] = '\0';
    return ui_live_apply(&s_table, &set, now_ms, NULL, &k_ops);
}

TEST_CASE("live: a new image is pinned and decoded with the lock released, then commits and shows", "[ui_live]")
{
    fresh();
    TEST_ASSERT_EQUAL(UI_LIVE_OK, apply(image_set("np", 1, "sha256:" ID_A), 1000));
    TEST_ASSERT_EQUAL_STRING("lock unlock pin decode lock show unlock lock unlock ", fake.trace);
    TEST_ASSERT_FALSE(fake.decoded_locked);
    TEST_ASSERT_EQUAL(1, fake.pins);
    TEST_ASSERT_EQUAL_STRING(ID_A, slot_of(0, "cover")->image);
    TEST_ASSERT_TRUE(slot_of(0, "cover")->dirty);
}

TEST_CASE("live: a heartbeat of the image a slot holds needs no pin and no decode", "[ui_live]")
{
    fresh();
    TEST_ASSERT_EQUAL(UI_LIVE_OK, apply(image_set("np", 1, "sha256:" ID_A), 1000));
    TEST_ASSERT_EQUAL(UI_LIVE_OK, apply(image_set("np", 2, "sha256:" ID_A), 1600));
    TEST_ASSERT_EQUAL_STRING("lock unlock lock unlock lock unlock ", fake.trace);
    TEST_ASSERT_EQUAL(1, fake.pins);
    TEST_ASSERT_EQUAL(1, fake.decodes);
    TEST_ASSERT_EQUAL_UINT32(2, slot_of(0, "cover")->n);
}

TEST_CASE("live: a new cover replaces the old one, whose pin is given back after the commit", "[ui_live]")
{
    fresh();
    TEST_ASSERT_EQUAL(UI_LIVE_OK, apply(image_set("np", 1, "sha256:" ID_A), 1000));
    TEST_ASSERT_EQUAL(UI_LIVE_OK, apply(image_set("np", 2, "sha256:" ID_B), 1600));
    TEST_ASSERT_EQUAL_STRING("lock unlock pin decode lock show unlock discard lock unlock unpin ", fake.trace);
    TEST_ASSERT_EQUAL(1, fake.pins);
    TEST_ASSERT_EQUAL(1, fake.pixels_live);
    TEST_ASSERT_EQUAL(UI_LIVE_OK, apply(image_set("np", 3, NULL), 2100));
    TEST_ASSERT_EQUAL_STRING("lock unlock lock clear unlock discard lock unlock unpin ", fake.trace);
    TEST_ASSERT_EQUAL(0, fake.pins);
    TEST_ASSERT_EQUAL(0, fake.pixels_live);
}

TEST_CASE("live: a refusal at the check pins and decodes nothing", "[ui_live]")
{
    fresh();
    TEST_ASSERT_EQUAL(UI_LIVE_OK, apply(image_set("np", 5, "sha256:" ID_A), 1000));
    TEST_ASSERT_EQUAL(UI_LIVE_SEQUENCE, apply(image_set("np", 4, "sha256:" ID_B), 1100));
    TEST_ASSERT_EQUAL_STRING("lock unlock ", fake.trace);
    TEST_ASSERT_EQUAL(UI_LIVE_OK, apply(image_set("np", 6, "sha256:" ID_A), 1100));
    TEST_ASSERT_EQUAL(UI_LIVE_RATE, apply(image_set("np", 7, "sha256:" ID_B), 1200));
    TEST_ASSERT_EQUAL_STRING("lock unlock ", fake.trace);
    TEST_ASSERT_EQUAL(1, fake.decodes);
    TEST_ASSERT_EQUAL(1, fake.pins);
}

TEST_CASE("live: an Asset the pin refuses, or one that fails to decode, commits nothing", "[ui_live]")
{
    fresh();
    fake.pin_result = UI_LIVE_ASSET;
    TEST_ASSERT_EQUAL(UI_LIVE_ASSET, apply(image_set("np", 1, "sha256:" ID_A), 1000));
    TEST_ASSERT_EQUAL_STRING("lock unlock pin ", fake.trace);
    TEST_ASSERT_FALSE(slot_of(0, "cover")->set);
    fake.pin_result = UI_LIVE_MEMORY;
    TEST_ASSERT_EQUAL(UI_LIVE_MEMORY, apply(image_set("np", 1, "sha256:" ID_A), 1000));
    fake.pin_result = UI_LIVE_OK;
    fake.decode_result = UI_LIVE_ASSET;
    TEST_ASSERT_EQUAL(UI_LIVE_ASSET, apply(image_set("np", 1, "sha256:" ID_A), 1000));
    TEST_ASSERT_EQUAL_STRING("lock unlock pin decode unpin ", fake.trace);
    TEST_ASSERT_EQUAL(0, fake.pins);
    TEST_ASSERT_FALSE(slot_of(0, "cover")->set);
    fake.decode_result = UI_LIVE_OK;
    TEST_ASSERT_EQUAL(UI_LIVE_OK, apply(image_set("np", 1, "sha256:" ID_A), 1000));
    TEST_ASSERT_EQUAL(1, fake.pins);
}

static void rebind_during_decode(void)
{
    ui_live_bind(&s_table, s_slots, 16, "builtin", k_pages, 1);
}

TEST_CASE("live: a slot replaced while its decode ran refuses with layout and frees the decode", "[ui_live]")
{
    fresh();
    TEST_ASSERT_EQUAL(UI_LIVE_OK, apply(image_set("np", 1, "sha256:" ID_A), 1000));
    fake.during_decode = rebind_during_decode;
    TEST_ASSERT_EQUAL(UI_LIVE_LAYOUT, apply(image_set("np", 2, "sha256:" ID_B), 1600));
    TEST_ASSERT_EQUAL(0, fake.pins);
    TEST_ASSERT_EQUAL(1, fake.pixels_live);
    TEST_ASSERT_FALSE(slot_of(0, "cover")->set);
    fake.during_decode = NULL;
    TEST_ASSERT_EQUAL(UI_LIVE_OK, apply(image_set("np", 2, "sha256:" ID_B), 2100));
    TEST_ASSERT_EQUAL(1, fake.pins);
}

TEST_CASE("live: a session reset gives back every pin before the next set", "[ui_live]")
{
    fresh();
    TEST_ASSERT_EQUAL(UI_LIVE_OK, apply(image_set("np", 1, "sha256:" ID_A), 1000));
    TEST_ASSERT_EQUAL(UI_LIVE_OK, apply(image_set("np2", 1, "sha256:" ID_A), 1000));
    TEST_ASSERT_EQUAL(2, fake.pins);
    ui_live_session(&s_table, 2000);
    fake.trace[0] = '\0';
    ui_live_release(&s_table, &k_ops);
    TEST_ASSERT_EQUAL_STRING("lock unlock unpin unpin ", fake.trace);
    TEST_ASSERT_EQUAL(0, fake.pins);
    TEST_ASSERT_EQUAL_UINT8(0, s_table.released_count);
    ui_live_release(&s_table, &k_ops);
    TEST_ASSERT_EQUAL(0, fake.pins);
}

TEST_CASE("live: text and numbers go through apply in one locked step", "[ui_live]")
{
    fresh();
    TEST_ASSERT_EQUAL(UI_LIVE_OK, apply(text_set("np", "title", 1, "Song"), 1000));
    TEST_ASSERT_EQUAL_STRING("lock unlock lock unlock ", fake.trace);
    TEST_ASSERT_EQUAL_STRING("Song", slot_of(0, "title")->text);
    TEST_ASSERT_EQUAL(0, fake.pins);
}
