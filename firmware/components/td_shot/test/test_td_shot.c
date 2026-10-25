#include <stddef.h>
#include <string.h>
#include "unity.h"
#include "td_shot.h"

#define MAX_LINES 10
#define PATTERN_W 64
#define PATTERN_ROWS 49
#define PATTERN_BYTES (PATTERN_W * PATTERN_ROWS * 2)
#define DECODED_MAX (MAX_LINES * TD_SHOT_LINE_RAW)

/* python -c "import zlib; print(zlib.crc32(bytes(((i*7+3)&0xFF) for i in range(64*48*2))))" */
#define PATTERN_64X48_CRC32 200789576u

#define ASSERT_ERR(want, got) TEST_ASSERT_EQUAL_INT((want), (got))

typedef struct {
    char lines[MAX_LINES][TD_SHOT_LINE_MAX + 1];
    size_t lens[MAX_LINES];
    unsigned calls;
    unsigned fail_at;
    bool violated;
} sink_rec_t;

static sink_rec_t s_sink;
static uint8_t s_pattern[PATTERN_BYTES];
static uint8_t s_expect[DECODED_MAX];
static uint8_t s_decoded[DECODED_MAX];

/* The stream frame is large and the Unity runner has little stack left, so the sink only flags a violation and stops. */
static bool rec_sink(void *ctx, const char *line, size_t len)
{
    sink_rec_t *s = ctx;
    if (s->calls >= MAX_LINES || len > TD_SHOT_LINE_MAX) {
        s->violated = true;
        return false;
    }
    memcpy(s->lines[s->calls], line, len);
    s->lens[s->calls] = len;
    s->calls++;
    return s->calls != s->fail_at;
}

/* Streams into s_sink, which rejects the fail_at-th line (0 for none). */
static esp_err_t run_stream(const td_shot_image_t *img, td_shot_scale_t scale, unsigned fail_at, uint32_t *crc)
{
    memset(&s_sink, 0, sizeof s_sink);
    s_sink.fail_at = fail_at;
    const esp_err_t err = td_shot_stream(img, scale, rec_sink, &s_sink, crc);
    TEST_ASSERT_FALSE_MESSAGE(s_sink.violated, "sink got an oversize line or more lines than the recorder holds");
    return err;
}

static void fill_pattern(void)
{
    for (size_t i = 0; i < PATTERN_BYTES; i++) {
        s_pattern[i] = (uint8_t)(i * 7 + 3);
    }
}

static int b64_value(char c)
{
    if (c >= 'A' && c <= 'Z') {
        return c - 'A';
    }
    if (c >= 'a' && c <= 'z') {
        return c - 'a' + 26;
    }
    if (c >= '0' && c <= '9') {
        return c - '0' + 52;
    }
    if (c == '+') {
        return 62;
    }
    if (c == '/') {
        return 63;
    }
    return -1;
}

static size_t decode_line(unsigned index, uint8_t *out)
{
    const char *line = s_sink.lines[index];
    const size_t len = s_sink.lens[index];
    const size_t prefix = strlen(TD_SHOT_LINE_PREFIX);

    TEST_ASSERT_TRUE_MESSAGE(len > prefix + 1, "line has no body");
    TEST_ASSERT_EQUAL_MEMORY(TD_SHOT_LINE_PREFIX, line, prefix);
    TEST_ASSERT_EQUAL_CHAR('\n', line[len - 1]);

    const char *body = line + prefix;
    const size_t body_len = len - prefix - 1;
    TEST_ASSERT_EQUAL_UINT32(0, body_len % 4);

    size_t n = 0;
    for (size_t i = 0; i < body_len; i += 4) {
        uint32_t group = 0;
        unsigned pad = 0;
        for (size_t k = 0; k < 4; k++) {
            const char c = body[i + k];
            if (c == '=') {
                TEST_ASSERT_TRUE_MESSAGE(i + 4 == body_len && k >= 2, "padding before the final group or in its first two slots");
                pad++;
                group <<= 6;
            } else {
                TEST_ASSERT_TRUE_MESSAGE(pad == 0, "data after padding");
                const int v = b64_value(c);
                TEST_ASSERT_TRUE_MESSAGE(v >= 0, "character outside the base64 alphabet");
                group = group << 6 | (uint32_t)v;
            }
        }
        out[n++] = (uint8_t)(group >> 16);
        if (pad < 2) {
            out[n++] = (uint8_t)(group >> 8);
        }
        if (pad < 1) {
            out[n++] = (uint8_t)group;
        }
    }
    return n;
}

static size_t decode_all(uint8_t *out)
{
    size_t total = 0;
    for (unsigned i = 0; i < s_sink.calls; i++) {
        total += decode_line(i, out + total);
    }
    return total;
}

static uint32_t crc32_ref(const uint8_t *data, size_t len)
{
    uint32_t crc = 0xFFFFFFFFu;
    for (size_t i = 0; i < len; i++) {
        crc ^= data[i];
        for (int bit = 0; bit < 8; bit++) {
            crc = (crc >> 1) ^ (0xEDB88320u & (0u - (crc & 1u)));
        }
    }
    return ~crc;
}

static size_t decimate_ref(const td_shot_image_t *img, unsigned scale, uint8_t *out)
{
    size_t n = 0;
    for (unsigned y = 0; y < img->height; y += scale) {
        for (unsigned x = 0; x < img->width; x += scale) {
            out[n++] = img->px[y * img->stride + x * 2];
            out[n++] = img->px[y * img->stride + x * 2 + 1];
        }
    }
    return n;
}

static void expect_layout(td_shot_layout_t got, uint16_t width, uint16_t height, uint32_t bytes, uint32_t lines)
{
    TEST_ASSERT_EQUAL_UINT32(width, got.width);
    TEST_ASSERT_EQUAL_UINT32(height, got.height);
    TEST_ASSERT_EQUAL_UINT32(bytes, got.bytes);
    TEST_ASSERT_EQUAL_UINT32(lines, got.lines);
}

static void expect_stream_bytes(const td_shot_image_t *img, td_shot_scale_t scale, const uint8_t *want, size_t want_len)
{
    uint32_t crc = 0;
    ASSERT_ERR(ESP_OK, run_stream(img, scale, 0, &crc));
    TEST_ASSERT_EQUAL_UINT32((want_len + TD_SHOT_LINE_RAW - 1) / TD_SHOT_LINE_RAW, s_sink.calls);
    TEST_ASSERT_EQUAL_UINT32(want_len, decode_all(s_decoded));
    TEST_ASSERT_EQUAL_HEX8_ARRAY(want, s_decoded, want_len);
    TEST_ASSERT_EQUAL_HEX32(crc32_ref(want, want_len), crc);
}

static void expect_single_line(const char *raw, uint16_t width, const char *want_line)
{
    const td_shot_image_t img = {.px = (const uint8_t *)raw, .width = width, .height = 1, .stride = (uint32_t)width * 2};
    uint32_t crc = 0;
    ASSERT_ERR(ESP_OK, run_stream(&img, TD_SHOT_SCALE_1, 0, &crc));
    TEST_ASSERT_EQUAL_UINT32(1, s_sink.calls);
    TEST_ASSERT_EQUAL_UINT32(strlen(want_line), s_sink.lens[0]);
    TEST_ASSERT_EQUAL_MEMORY(want_line, s_sink.lines[0], strlen(want_line));
}

TEST_CASE("parse_scale takes NULL, 1, 2 and 4 and nothing else", "[td_shot]")
{
    td_shot_scale_t scale = TD_SHOT_SCALE_4;
    TEST_ASSERT_TRUE(td_shot_parse_scale(NULL, &scale));
    TEST_ASSERT_EQUAL_INT(TD_SHOT_SCALE_1, scale);

    TEST_ASSERT_TRUE(td_shot_parse_scale("1", &scale));
    TEST_ASSERT_EQUAL_INT(TD_SHOT_SCALE_1, scale);
    TEST_ASSERT_TRUE(td_shot_parse_scale("2", &scale));
    TEST_ASSERT_EQUAL_INT(TD_SHOT_SCALE_2, scale);
    TEST_ASSERT_TRUE(td_shot_parse_scale("4", &scale));
    TEST_ASSERT_EQUAL_INT(TD_SHOT_SCALE_4, scale);

    const char *const rejected[] = {"0", "3", "8", "", "2x", " 2"};
    for (size_t i = 0; i < sizeof rejected / sizeof rejected[0]; i++) {
        TEST_ASSERT_FALSE_MESSAGE(td_shot_parse_scale(rejected[i], &scale), rejected[i]);
    }
    TEST_ASSERT_FALSE(td_shot_parse_scale("2", NULL));
}

TEST_CASE("layout of the 1024x600 screen at each scale", "[td_shot]")
{
    const td_shot_image_t screen = {.px = NULL, .width = 1024, .height = 600, .stride = 2048};
    expect_layout(td_shot_layout(&screen, TD_SHOT_SCALE_1), 1024, 600, 1228800, 1600);
    expect_layout(td_shot_layout(&screen, TD_SHOT_SCALE_2), 512, 300, 307200, 400);
    expect_layout(td_shot_layout(&screen, TD_SHOT_SCALE_4), 256, 150, 76800, 100);
}

TEST_CASE("layout of a tiny image and of line boundaries", "[td_shot]")
{
    const td_shot_image_t tiny = {.px = NULL, .width = 8, .height = 4, .stride = 16};
    expect_layout(td_shot_layout(&tiny, TD_SHOT_SCALE_1), 8, 4, 64, 1);
    expect_layout(td_shot_layout(&tiny, TD_SHOT_SCALE_2), 4, 2, 16, 1);
    expect_layout(td_shot_layout(&tiny, TD_SHOT_SCALE_4), 2, 1, 4, 1);

    const td_shot_image_t exact = {.px = NULL, .width = 384, .height = 1, .stride = 768};
    expect_layout(td_shot_layout(&exact, TD_SHOT_SCALE_1), 384, 1, 768, 1);
    const td_shot_image_t one_over = {.px = NULL, .width = 385, .height = 1, .stride = 770};
    expect_layout(td_shot_layout(&one_over, TD_SHOT_SCALE_1), 385, 1, 770, 2);
}

TEST_CASE("layout is all zero for input the stream would reject", "[td_shot]")
{
    const td_shot_image_t tiny = {.px = NULL, .width = 8, .height = 4, .stride = 16};
    expect_layout(td_shot_layout(NULL, TD_SHOT_SCALE_1), 0, 0, 0, 0);
    expect_layout(td_shot_layout(&tiny, (td_shot_scale_t)3), 0, 0, 0, 0);
    expect_layout(td_shot_layout(&tiny, (td_shot_scale_t)0), 0, 0, 0, 0);

    const td_shot_image_t ragged = {.px = NULL, .width = 6, .height = 4, .stride = 12};
    expect_layout(td_shot_layout(&ragged, TD_SHOT_SCALE_4), 0, 0, 0, 0);

    const td_shot_image_t huge = {.px = NULL, .width = 65535, .height = 65535, .stride = 131070};
    expect_layout(td_shot_layout(&huge, TD_SHOT_SCALE_1), 0, 0, 0, 0);

    /* 4294967292 bytes fit in 32 bits, but rounding them up to whole lines would wrap to 0 lines. */
    const td_shot_image_t wraps = {.px = NULL, .width = 32769, .height = 65534, .stride = 65538};
    expect_layout(td_shot_layout(&wraps, TD_SHOT_SCALE_1), 0, 0, 0, 0);
}

TEST_CASE("base64 follows RFC 4648 padding on the final chunk", "[td_shot]")
{
    expect_single_line("fo", 1, "@tdb Zm8=\n");
    expect_single_line("foob", 2, "@tdb Zm9vYg==\n");
    expect_single_line("foobar", 3, "@tdb Zm9vYmFy\n");
}

TEST_CASE("an 8x4 ramp keeps the top-left pixel of each block at every scale", "[td_shot]")
{
    uint8_t ramp[64];
    for (size_t i = 0; i < sizeof ramp; i++) {
        ramp[i] = (uint8_t)i;
    }
    const td_shot_image_t img = {.px = ramp, .width = 8, .height = 4, .stride = 16};

    const uint8_t want2[] = {0, 1, 4, 5, 8, 9, 12, 13, 32, 33, 36, 37, 40, 41, 44, 45};
    const uint8_t want4[] = {0, 1, 8, 9};

    expect_stream_bytes(&img, TD_SHOT_SCALE_1, ramp, sizeof ramp);
    expect_stream_bytes(&img, TD_SHOT_SCALE_2, want2, sizeof want2);
    expect_stream_bytes(&img, TD_SHOT_SCALE_4, want4, sizeof want4);
}

TEST_CASE("rows are read through the stride and padding never reaches the output", "[td_shot]")
{
    uint8_t buf[4 * 12];
    for (unsigned y = 0; y < 4; y++) {
        for (unsigned i = 0; i < 12; i++) {
            buf[y * 12 + i] = i < 8 ? (uint8_t)(0x10 * y + i) : 0xEE;
        }
    }
    const td_shot_image_t img = {.px = buf, .width = 4, .height = 4, .stride = 12};

    const uint8_t want1[] = {0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x10, 0x11, 0x12,
                             0x13, 0x14, 0x15, 0x16, 0x17, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25,
                             0x26, 0x27, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37};
    const uint8_t want2[] = {0x00, 0x01, 0x04, 0x05, 0x20, 0x21, 0x24, 0x25};

    expect_stream_bytes(&img, TD_SHOT_SCALE_1, want1, sizeof want1);
    expect_stream_bytes(&img, TD_SHOT_SCALE_2, want2, sizeof want2);
}

TEST_CASE("a multi-line stream has full 1024-character lines", "[td_shot]")
{
    fill_pattern();
    const td_shot_image_t img = {.px = s_pattern, .width = PATTERN_W, .height = 48, .stride = PATTERN_W * 2};
    uint32_t crc = 0;
    ASSERT_ERR(ESP_OK, run_stream(&img, TD_SHOT_SCALE_1, 0, &crc));

    TEST_ASSERT_EQUAL_UINT32(td_shot_layout(&img, TD_SHOT_SCALE_1).lines, s_sink.calls);
    TEST_ASSERT_EQUAL_UINT32(8, s_sink.calls);
    for (unsigned i = 0; i < s_sink.calls; i++) {
        TEST_ASSERT_EQUAL_UINT32(strlen(TD_SHOT_LINE_PREFIX) + 1024 + 1, s_sink.lens[i]);
    }
    TEST_ASSERT_EQUAL_UINT32(PATTERN_W * 48 * 2, decode_all(s_decoded));
    TEST_ASSERT_EQUAL_HEX8_ARRAY(s_pattern, s_decoded, PATTERN_W * 48 * 2);
    TEST_ASSERT_EQUAL_HEX32(PATTERN_64X48_CRC32, crc);
}

TEST_CASE("a short final line carries its own padding", "[td_shot]")
{
    fill_pattern();
    const td_shot_image_t img = {.px = s_pattern, .width = PATTERN_W, .height = PATTERN_ROWS, .stride = PATTERN_W * 2};
    uint32_t crc = 0;
    ASSERT_ERR(ESP_OK, run_stream(&img, TD_SHOT_SCALE_1, 0, &crc));

    TEST_ASSERT_EQUAL_UINT32(9, s_sink.calls);
    for (unsigned i = 0; i < 8; i++) {
        TEST_ASSERT_EQUAL_UINT32(strlen(TD_SHOT_LINE_PREFIX) + 1024 + 1, s_sink.lens[i]);
    }
    /* 128 bytes is 42 whole groups plus 2 bytes, so 43 groups of 4 characters and one '='. */
    TEST_ASSERT_EQUAL_UINT32(strlen(TD_SHOT_LINE_PREFIX) + 172 + 1, s_sink.lens[8]);
    TEST_ASSERT_EQUAL_CHAR('=', s_sink.lines[8][s_sink.lens[8] - 2]);
    TEST_ASSERT_EQUAL_UINT32(PATTERN_BYTES, decode_all(s_decoded));
    TEST_ASSERT_EQUAL_HEX8_ARRAY(s_pattern, s_decoded, PATTERN_BYTES);
}

TEST_CASE("crc32 equals python zlib.crc32 for a pinned pattern", "[td_shot]")
{
    TEST_ASSERT_EQUAL_HEX32(0xCBF43926u, crc32_ref((const uint8_t *)"123456789", 9));
    fill_pattern();
    TEST_ASSERT_EQUAL_HEX32(PATTERN_64X48_CRC32, crc32_ref(s_pattern, PATTERN_W * 48 * 2));

    const td_shot_image_t img = {.px = s_pattern, .width = PATTERN_W, .height = 48, .stride = PATTERN_W * 2};
    uint32_t crc = 0;
    ASSERT_ERR(ESP_OK, run_stream(&img, TD_SHOT_SCALE_1, 0, &crc));
    TEST_ASSERT_EQUAL_HEX32(PATTERN_64X48_CRC32, crc);
}

TEST_CASE("crc32 and bytes match an independent decimation at scales 1, 2 and 4", "[td_shot]")
{
    fill_pattern();
    const td_shot_image_t img = {.px = s_pattern, .width = PATTERN_W, .height = 48, .stride = PATTERN_W * 2};
    const td_shot_scale_t scales[] = {TD_SHOT_SCALE_1, TD_SHOT_SCALE_2, TD_SHOT_SCALE_4};

    for (size_t i = 0; i < sizeof scales / sizeof scales[0]; i++) {
        const size_t want_len = decimate_ref(&img, (unsigned)scales[i], s_expect);
        TEST_ASSERT_EQUAL_UINT32(td_shot_layout(&img, scales[i]).bytes, want_len);
        expect_stream_bytes(&img, scales[i], s_expect, want_len);
    }
}

TEST_CASE("a sink failure on the second line aborts with ESP_FAIL", "[td_shot]")
{
    fill_pattern();
    const td_shot_image_t img = {.px = s_pattern, .width = PATTERN_W, .height = 48, .stride = PATTERN_W * 2};
    uint32_t crc = 0xDEADBEEFu;
    ASSERT_ERR(ESP_FAIL, run_stream(&img, TD_SHOT_SCALE_1, 2, &crc));
    TEST_ASSERT_EQUAL_UINT32(2, s_sink.calls);
    TEST_ASSERT_EQUAL_HEX32(crc32_ref(s_pattern, TD_SHOT_LINE_RAW), crc);
}

TEST_CASE("a sink that rejects the first or the short final line stops there", "[td_shot]")
{
    fill_pattern();
    const td_shot_image_t img = {.px = s_pattern, .width = PATTERN_W, .height = PATTERN_ROWS, .stride = PATTERN_W * 2};
    uint32_t crc = 0xDEADBEEFu;
    ASSERT_ERR(ESP_FAIL, run_stream(&img, TD_SHOT_SCALE_1, 1, &crc));
    TEST_ASSERT_EQUAL_UINT32(1, s_sink.calls);
    TEST_ASSERT_EQUAL_HEX32(0, crc);

    ASSERT_ERR(ESP_FAIL, run_stream(&img, TD_SHOT_SCALE_1, 9, &crc));
    TEST_ASSERT_EQUAL_UINT32(9, s_sink.calls);
    TEST_ASSERT_EQUAL_HEX32(crc32_ref(s_pattern, 8 * TD_SHOT_LINE_RAW), crc);
}

TEST_CASE("an image with no pixels streams nothing and has CRC 0", "[td_shot]")
{
    const td_shot_image_t empty = {.px = s_pattern, .width = 0, .height = 0, .stride = 0};
    uint32_t crc = 0xDEADBEEFu;
    ASSERT_ERR(ESP_OK, run_stream(&empty, TD_SHOT_SCALE_1, 0, &crc));
    TEST_ASSERT_EQUAL_UINT32(0, s_sink.calls);
    TEST_ASSERT_EQUAL_HEX32(0, crc);
}

TEST_CASE("bad arguments return ESP_ERR_INVALID_ARG without calling the sink", "[td_shot]")
{
    fill_pattern();
    const td_shot_image_t good = {.px = s_pattern, .width = PATTERN_W, .height = 48, .stride = PATTERN_W * 2};
    td_shot_image_t bad = good;
    uint32_t crc = 0x12345678u;
    memset(&s_sink, 0, sizeof s_sink);

    ASSERT_ERR(ESP_ERR_INVALID_ARG, td_shot_stream(NULL, TD_SHOT_SCALE_1, rec_sink, &s_sink, &crc));
    ASSERT_ERR(ESP_ERR_INVALID_ARG, td_shot_stream(&good, TD_SHOT_SCALE_1, NULL, &s_sink, &crc));
    ASSERT_ERR(ESP_ERR_INVALID_ARG, td_shot_stream(&good, TD_SHOT_SCALE_1, rec_sink, &s_sink, NULL));

    bad.px = NULL;
    ASSERT_ERR(ESP_ERR_INVALID_ARG, td_shot_stream(&bad, TD_SHOT_SCALE_1, rec_sink, &s_sink, &crc));

    ASSERT_ERR(ESP_ERR_INVALID_ARG, td_shot_stream(&good, (td_shot_scale_t)3, rec_sink, &s_sink, &crc));
    ASSERT_ERR(ESP_ERR_INVALID_ARG, td_shot_stream(&good, (td_shot_scale_t)0, rec_sink, &s_sink, &crc));

    bad = good;
    bad.width = 66;
    ASSERT_ERR(ESP_ERR_INVALID_ARG, td_shot_stream(&bad, TD_SHOT_SCALE_4, rec_sink, &s_sink, &crc));
    bad = good;
    bad.height = 50;
    ASSERT_ERR(ESP_ERR_INVALID_ARG, td_shot_stream(&bad, TD_SHOT_SCALE_4, rec_sink, &s_sink, &crc));

    bad = good;
    bad.stride = PATTERN_W * 2 - 1;
    ASSERT_ERR(ESP_ERR_INVALID_ARG, td_shot_stream(&bad, TD_SHOT_SCALE_1, rec_sink, &s_sink, &crc));

    TEST_ASSERT_EQUAL_UINT32(0, s_sink.calls);
    TEST_ASSERT_EQUAL_HEX32(0x12345678u, crc);
}
