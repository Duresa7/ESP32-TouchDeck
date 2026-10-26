#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Copies the logical rect x1..x2 by y1..y2 of a screen turned a quarter turn into a panel frame buffer that is
 * phys_w by phys_h RGB565 pixels with rows phys_w pixels apart. src holds the rect's top-left pixel, with rows
 * src_stride pixels apart. Clockwise matches esp_lvgl_adapter's ROTATE_90, where logical (x, y) lands at panel
 * row x, column phys_w - 1 - y. Counter-clockwise matches ROTATE_270, where it lands at panel row phys_h - 1 - x,
 * column y.
 *
 * It walks the rect in tiles small enough that the panel rows one tile writes, 2 KB apart in PSRAM, stay in the
 * data cache until the tile has filled whole cache lines of them.
 */
void td_rotate565_region(const uint16_t *src, size_t src_stride, uint16_t *fb, int phys_w, int phys_h, int x1, int y1,
                         int x2, int y2, bool clockwise);

#ifdef __cplusplus
}
#endif
