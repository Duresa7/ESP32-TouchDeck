#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include "unity.h"
#include "td_rotate.h"

/* A panel small enough to check whole, and tall enough that the rects below cross the 16 by 128 tile edges. The
 * logical screen is PHYS_H by PHYS_W. */
#define PHYS_W 160
#define PHYS_H 24
#define SENTINEL 0xBEEF

static uint16_t s_src[PHYS_W * (PHYS_H + 5)];
static uint16_t s_got[PHYS_W * PHYS_H];
static uint16_t s_want[PHYS_W * PHYS_H];

/* esp_lvgl_adapter 0.7.3's display_rotate_copy_region, one pixel at a time: ROTATE_90 puts logical (gx, gy) at
 * panel row gx, column phys_w - 1 - gy, and ROTATE_270 at row ver_res - 1 - gx, column gy. */
static void reference(const uint16_t *src, int stride, int x1, int y1, int x2, int y2, bool clockwise)
{
    for (int y = y1; y <= y2; y++) {
        for (int x = x1; x <= x2; x++) {
            const int row = clockwise ? x : PHYS_H - 1 - x;
            const int col = clockwise ? PHYS_W - 1 - y : y;
            s_want[row * PHYS_W + col] = src[(y - y1) * stride + (x - x1)];
        }
    }
}

static void check_rect(int x1, int y1, int x2, int y2, bool clockwise)
{
    const int w = x2 - x1 + 1;
    const int stride = w + 5;
    for (size_t i = 0; i < sizeof(s_src) / sizeof(s_src[0]); i++) {
        s_src[i] = (uint16_t)(i * 7 + 1);
    }
    for (size_t i = 0; i < PHYS_W * PHYS_H; i++) {
        s_got[i] = SENTINEL;
        s_want[i] = SENTINEL;
    }
    reference(s_src, stride, x1, y1, x2, y2, clockwise);
    td_rotate565_region(s_src, (size_t)stride, s_got, PHYS_W, PHYS_H, x1, y1, x2, y2, clockwise);
    TEST_ASSERT_EQUAL_HEX16_ARRAY(s_want, s_got, PHYS_W * PHYS_H);
}

TEST_CASE("clockwise rotation matches the adapter's ROTATE_90 mapping", "[td_rotate]")
{
    check_rect(0, 0, PHYS_H - 1, PHYS_W - 1, true);
    check_rect(1, 2, 22, 150, true);
    check_rect(5, 7, 5, 7, true);
    check_rect(0, 33, PHYS_H - 1, 34, true);
}

TEST_CASE("counter-clockwise rotation matches the adapter's ROTATE_270 mapping", "[td_rotate]")
{
    check_rect(0, 0, PHYS_H - 1, PHYS_W - 1, false);
    check_rect(1, 2, 22, 150, false);
    check_rect(5, 7, 5, 7, false);
    check_rect(17, 0, 18, PHYS_W - 1, false);
}

TEST_CASE("clockwise rotation puts the logical corners where portrait expects them", "[td_rotate]")
{
    /* Logical top-left lands at the panel's top-right, so the screen reads upright with the glass turned clockwise. */
    static uint16_t src[PHYS_H * PHYS_W];
    for (int i = 0; i < PHYS_H * PHYS_W; i++) {
        src[i] = 0;
    }
    src[0] = 1;                                   /* logical (0, 0) */
    src[PHYS_H - 1] = 2;                          /* logical (23, 0) */
    src[(PHYS_W - 1) * PHYS_H] = 3;               /* logical (0, 159) */
    src[(PHYS_W - 1) * PHYS_H + PHYS_H - 1] = 4;  /* logical (23, 159) */
    td_rotate565_region(src, PHYS_H, s_got, PHYS_W, PHYS_H, 0, 0, PHYS_H - 1, PHYS_W - 1, true);
    TEST_ASSERT_EQUAL_HEX16(1, s_got[0 * PHYS_W + PHYS_W - 1]);
    TEST_ASSERT_EQUAL_HEX16(2, s_got[(PHYS_H - 1) * PHYS_W + PHYS_W - 1]);
    TEST_ASSERT_EQUAL_HEX16(3, s_got[0 * PHYS_W + 0]);
    TEST_ASSERT_EQUAL_HEX16(4, s_got[(PHYS_H - 1) * PHYS_W + 0]);
}

TEST_CASE("an empty rect writes nothing", "[td_rotate]")
{
    for (size_t i = 0; i < PHYS_W * PHYS_H; i++) {
        s_got[i] = SENTINEL;
    }
    td_rotate565_region(s_src, 4, s_got, PHYS_W, PHYS_H, 3, 3, 2, 3, true);
    for (size_t i = 0; i < PHYS_W * PHYS_H; i++) {
        TEST_ASSERT_EQUAL_HEX16(SENTINEL, s_got[i]);
    }
}

/*
 * The cases below hold the tiled rotate to a naive per-pixel rotate, in both directions, on seeded random pixels. They
 * compare the whole panel, so a stray write outside the rect fails too, and one guard pixel past the end.
 */
typedef struct {
    int pw, ph;          /* panel; the logical screen is ph by pw */
    int x, y, w, h;      /* logical rect */
    int pad;             /* source stride beyond w */
    int src_off, fb_off; /* a 1 moves the buffer off word alignment */
    int strip;           /* source rows per call, as LVGL's partial flushes; 0 for one call */
} rect_case_t;

static uint32_t next_random(uint32_t *seed)
{
    *seed = *seed * 1664525U + 1013904223U;
    return *seed;
}

static void check_random_rect(rect_case_t c)
{
    const size_t stride = (size_t)(c.w + c.pad);
    const size_t src_count = stride * (size_t)c.h + 1;
    const size_t fb_count = (size_t)c.pw * (size_t)c.ph + 1;
    uint16_t *source = malloc(src_count * sizeof *source);
    uint16_t *got = malloc(fb_count * sizeof *got);
    uint16_t *want = malloc(fb_count * sizeof *want);
    if (source == NULL || got == NULL || want == NULL) {
        free(source);
        free(got);
        free(want);
        TEST_FAIL_MESSAGE("out of memory");
        return;
    }
    uint32_t seed = 0x5eed1234U;
    for (size_t i = 0; i < src_count; i++) {
        source[i] = (uint16_t)(next_random(&seed) >> 16);
    }
    const uint16_t *src = source + c.src_off;
    const int strip = c.strip > 0 ? c.strip : c.h;
    size_t bad = fb_count;
    bool bad_cw = false;
    for (int cw = 0; cw < 2 && bad == fb_count; cw++) {
        for (size_t i = 0; i < fb_count; i++) {
            got[i] = SENTINEL;
            want[i] = SENTINEL;
        }
        for (int sy = 0; sy < c.h; sy++) {
            for (int sx = 0; sx < c.w; sx++) {
                const int row = cw ? c.x + sx : c.ph - 1 - (c.x + sx);
                const int col = cw ? c.pw - 1 - (c.y + sy) : c.y + sy;
                want[c.fb_off + (size_t)row * (size_t)c.pw + (size_t)col] = src[(size_t)sy * stride + (size_t)sx];
            }
        }
        for (int sy = 0; sy < c.h; sy += strip) {
            const int last = sy + strip < c.h ? sy + strip - 1 : c.h - 1;
            td_rotate565_region(src + (size_t)sy * stride, stride, got + c.fb_off, c.pw, c.ph, c.x, c.y + sy,
                                c.x + c.w - 1, c.y + last, cw != 0);
        }
        for (size_t i = 0; i < fb_count; i++) {
            if (got[i] != want[i]) {
                bad = i;
                bad_cw = cw != 0;
                break;
            }
        }
    }
    const uint16_t bad_got = bad < fb_count ? got[bad] : 0;
    const uint16_t bad_want = bad < fb_count ? want[bad] : 0;
    free(source);
    free(got);
    free(want);
    if (bad < fb_count) {
        char message[256];
        snprintf(message, sizeof message,
                 "%s, panel %dx%d, rect %d,%d %dx%d, pad %d, src+%d, fb+%d, strip %d: first difference at pixel %u",
                 bad_cw ? "clockwise" : "counter-clockwise", c.pw, c.ph, c.x, c.y, c.w, c.h, c.pad, c.src_off,
                 c.fb_off, c.strip, (unsigned)bad);
        TEST_ASSERT_EQUAL_HEX16_MESSAGE(bad_want, bad_got, message);
    }
}

TEST_CASE("odd sizes, odd starts, padded strides and misaligned buffers match the naive rotate", "[td_rotate]")
{
    for (int src_off = 0; src_off < 2; src_off++) {
        for (int fb_off = 0; fb_off < 2; fb_off++) {
            for (int pad = 0; pad < 3; pad++) {
                check_random_rect((rect_case_t){ 160, 40, 1, 3, 33, 131, pad, src_off, fb_off, 0 });
                check_random_rect((rect_case_t){ 160, 40, 3, 1, 32, 130, pad, src_off, fb_off, 0 });
                check_random_rect((rect_case_t){ 160, 40, 2, 2, 31, 129, pad, src_off, fb_off, 0 });
                /* An odd panel width starts neighbouring panel rows on different word alignments. */
                check_random_rect((rect_case_t){ 159, 39, 1, 1, 37, 157, pad, src_off, fb_off, 0 });
                check_random_rect((rect_case_t){ 159, 39, 0, 0, 39, 159, pad, src_off, fb_off, 0 });
            }
        }
    }
}

TEST_CASE("one-pixel rows and columns match the naive rotate", "[td_rotate]")
{
    check_random_rect((rect_case_t){ 160, 40, 3, 5, 1, 1, 0, 0, 0, 0 });
    check_random_rect((rect_case_t){ 160, 40, 4, 6, 1, 1, 0, 1, 1, 0 });
    check_random_rect((rect_case_t){ 160, 40, 1, 7, 37, 1, 1, 0, 1, 0 });
    check_random_rect((rect_case_t){ 160, 40, 0, 8, 40, 1, 0, 1, 0, 0 });
    check_random_rect((rect_case_t){ 160, 40, 7, 1, 1, 157, 2, 1, 0, 0 });
    check_random_rect((rect_case_t){ 160, 40, 8, 0, 1, 160, 0, 0, 1, 0 });
    check_random_rect((rect_case_t){ 160, 40, 9, 2, 2, 1, 0, 0, 0, 0 });
    check_random_rect((rect_case_t){ 160, 40, 9, 3, 1, 2, 0, 0, 0, 0 });
}

TEST_CASE("rects around the 16x128 tile edges match the naive rotate", "[td_rotate]")
{
    for (int w = 15; w <= 17; w++) {
        for (int h = 127; h <= 129; h++) {
            check_random_rect((rect_case_t){ 160, 40, 15, 15, w, h, 3, 0, 0, 0 });
            check_random_rect((rect_case_t){ 160, 40, 0, 0, w, h, 0, 0, 0, 0 });
            check_random_rect((rect_case_t){ 160, 40, 16, 31, w, h, 1, 1, 1, 0 });
        }
    }
    uint32_t seed = 0xc001cafeU;
    for (int i = 0; i < 64; i++) {
        const int x = (int)(next_random(&seed) % 40);
        const int y = (int)(next_random(&seed) % 160);
        const int w = 1 + (int)(next_random(&seed) % (uint32_t)(40 - x));
        const int h = 1 + (int)(next_random(&seed) % (uint32_t)(160 - y));
        check_random_rect((rect_case_t){ 160, 40, x, y, w, h, i % 4, i % 2, (i / 2) % 2, (i % 3) * 37 });
    }
}
