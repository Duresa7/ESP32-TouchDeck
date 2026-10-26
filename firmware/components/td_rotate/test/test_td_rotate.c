#include <stdbool.h>
#include <stdint.h>
#include "unity.h"
#include "td_rotate.h"

/* A small panel, so every rect below crosses the 16 by 32 tile edges. The logical screen is PHYS_H by PHYS_W. */
#define PHYS_W 40
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
    check_rect(1, 2, 22, 38, true);
    check_rect(5, 7, 5, 7, true);
    check_rect(0, 33, PHYS_H - 1, 34, true);
}

TEST_CASE("counter-clockwise rotation matches the adapter's ROTATE_270 mapping", "[td_rotate]")
{
    check_rect(0, 0, PHYS_H - 1, PHYS_W - 1, false);
    check_rect(1, 2, 22, 38, false);
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
    src[(PHYS_W - 1) * PHYS_H] = 3;               /* logical (0, 39) */
    src[(PHYS_W - 1) * PHYS_H + PHYS_H - 1] = 4;  /* logical (23, 39) */
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
