#include <stdint.h>
#include "esp_lv_adapter_display.h"
#include "td_rotate.h"

/*
 * esp_lvgl_adapter 0.7.3 rotates each flushed area in display_rotate_copy_region, one pixel at a time down a
 * column of 64 panel rows. Those rows are 2 KB apart in PSRAM and land in 4 sets of the data cache, 16 rows a set
 * against 8 ways, so nearly every pixel write misses and portrait ran at under 1 frame a second. The linker wraps
 * the adapter's calls to it (see CMakeLists.txt), and this sends the RGB565 quarter turns to td_rotate's tiled copy
 * and everything else back to the adapter. The arguments are the adapter's own; the CMake check pins the version.
 */
void __real_display_rotate_copy_region(const void *src, void *dst_fb, uint16_t lv_x_start, uint16_t lv_y_start,
                                       uint16_t lv_x_end, uint16_t lv_y_end, uint16_t src_stride_px,
                                       uint16_t hor_res, uint16_t ver_res, esp_lv_adapter_rotation_t rotation,
                                       uint8_t color_bytes, int block_size_small, int block_size_large);

void __wrap_display_rotate_copy_region(const void *src, void *dst_fb, uint16_t lv_x_start, uint16_t lv_y_start,
                                       uint16_t lv_x_end, uint16_t lv_y_end, uint16_t src_stride_px,
                                       uint16_t hor_res, uint16_t ver_res, esp_lv_adapter_rotation_t rotation,
                                       uint8_t color_bytes, int block_size_small, int block_size_large);

void __wrap_display_rotate_copy_region(const void *src, void *dst_fb, uint16_t lv_x_start, uint16_t lv_y_start,
                                       uint16_t lv_x_end, uint16_t lv_y_end, uint16_t src_stride_px,
                                       uint16_t hor_res, uint16_t ver_res, esp_lv_adapter_rotation_t rotation,
                                       uint8_t color_bytes, int block_size_small, int block_size_large)
{
    const bool quarter = rotation == ESP_LV_ADAPTER_ROTATE_90 || rotation == ESP_LV_ADAPTER_ROTATE_270;
    if (!quarter || color_bytes != 2) {
        __real_display_rotate_copy_region(src, dst_fb, lv_x_start, lv_y_start, lv_x_end, lv_y_end, src_stride_px,
                                          hor_res, ver_res, rotation, color_bytes, block_size_small,
                                          block_size_large);
        return;
    }
    td_rotate565_region(src, src_stride_px, dst_fb, hor_res, ver_res, lv_x_start, lv_y_start, lv_x_end, lv_y_end,
                        rotation == ESP_LV_ADAPTER_ROTATE_90);
}
