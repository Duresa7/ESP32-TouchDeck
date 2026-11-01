#pragma once
#include "td_layout.h"
#define LV_SYMBOL_PREV "prev"
#define LV_SYMBOL_PLAY "play"
#define LV_SYMBOL_PAUSE "pause"
#define LV_SYMBOL_NEXT "next"
#define LV_SYMBOL_MUTE "mute"
#define LV_SYMBOL_VOLUME_MID "mid"
#define LV_SYMBOL_VOLUME_MAX "max"
#define LV_IMAGE_HEADER_MAGIC 0x19
#define LV_COLOR_FORMAT_RGB565A8 1
typedef struct {
    struct { unsigned magic, cf, w, h, stride; } header;
    size_t data_size;
    const uint8_t *data;
} lv_image_dsc_t;
typedef struct { uint16_t loaded, missing; } ui_icon_counts_t;
