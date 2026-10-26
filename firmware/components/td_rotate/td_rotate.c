#include "td_rotate.h"

/* A tile is TILE_W source columns, which become TILE_W panel rows, by up to TILE_H source rows, which become one
 * contiguous run of each panel row. Panel rows 2 KB apart share 4 of the 128 sets of the S3's 64 KB 8-way data cache,
 * so 16 of them, 4 lines each, fit beside the tile's source lines. LVGL's partial flushes are shorter than TILE_H, so
 * a tile spans a whole flush. On the board this measured 11 fps in portrait against 8 fps with 32-row tiles. */
#define TILE_W 16
#define TILE_H 128

static int min_int(int a, int b)
{
    return a < b ? a : b;
}

void td_rotate565_region(const uint16_t *src, size_t src_stride, uint16_t *fb, int phys_w, int phys_h, int x1, int y1,
                         int x2, int y2, bool clockwise)
{
    const int w = x2 - x1 + 1;
    const int h = y2 - y1 + 1;
    if (w <= 0 || h <= 0) {
        return;
    }
    const size_t pitch = (size_t)phys_w;

    for (int ty = 0; ty < h; ty += TILE_H) {
        const int ty_end = min_int(ty + TILE_H, h);
        for (int tx = 0; tx < w; tx += TILE_W) {
            const int tx_end = min_int(tx + TILE_W, w);
            for (int x = tx; x < tx_end; x++) {
                const uint16_t *s = src + (size_t)ty * src_stride + (size_t)x;
                if (clockwise) {
                    uint16_t *d = fb + (size_t)(x1 + x) * pitch + (size_t)(phys_w - 1 - (y1 + ty));
                    for (int y = ty; y < ty_end; y++) {
                        *d-- = *s;
                        s += src_stride;
                    }
                } else {
                    uint16_t *d = fb + (size_t)(phys_h - 1 - (x1 + x)) * pitch + (size_t)(y1 + ty);
                    for (int y = ty; y < ty_end; y++) {
                        *d++ = *s;
                        s += src_stride;
                    }
                }
            }
        }
    }
}
