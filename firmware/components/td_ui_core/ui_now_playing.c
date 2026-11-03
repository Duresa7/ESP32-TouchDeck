#include "ui_now_playing.h"

const char *ui_part_name(ui_part_t part)
{
    switch (part) {
    case UI_PART_PREVIOUS:
        return "previous";
    case UI_PART_PLAY_PAUSE:
        return "play_pause";
    case UI_PART_NEXT:
        return "next";
    case UI_PART_MUTE:
        return "mute";
    default:
        return NULL;
    }
}

ui_np_state_t ui_np_state(double value)
{
    if (value == 1) {
        return UI_NP_PAUSED;
    }
    return value == 2 ? UI_NP_PLAYING : UI_NP_IDLE;
}

ui_np_layout_t ui_np_layout(int16_t w, int16_t h)
{
    ui_np_layout_t layout = { 0 };
    int size = h - 2 * UI_NP_PAD;
    const int by_width = (w - 2 * UI_NP_PAD) * 2 / 5;
    if (by_width < size) {
        size = by_width;
    }
    if (size > UI_NP_ART_MAX) {
        size = UI_NP_ART_MAX;
    }
    layout.has_art = w >= 2 * UI_NP_PAD && size >= UI_NP_ART_MIN;
    if (layout.has_art) {
        layout.art = (ui_rect_t){ UI_NP_PAD, (int16_t)((h - size) / 2), (int16_t)size, (int16_t)size };
    }
    layout.text_x = (int16_t)(layout.has_art ? UI_NP_PAD + size + UI_NP_PAD : UI_NP_PAD);
    const int text_w = w - UI_NP_PAD - layout.text_x;
    layout.text_w = (int16_t)(text_w > 0 ? text_w : 0);
    layout.controls = h >= UI_NP_ROW_MIN_TILE_H && layout.text_w >= UI_NP_ROW_MIN_TEXT_W;
    if (!layout.controls) {
        layout.part_count = 1;
        layout.parts[0] = UI_PART_PLAY_PAUSE;
        layout.zones[0] = (ui_rect_t){ 0, 0, w, h };
        return layout;
    }
    static const ui_part_t k_row[UI_NP_PARTS_MAX] = { UI_PART_PREVIOUS, UI_PART_PLAY_PAUSE, UI_PART_NEXT };
    const int16_t part_w = (int16_t)(layout.text_w / UI_NP_PARTS_MAX);
    const int16_t y = (int16_t)(h - UI_NP_PAD - UI_NP_ROW_H);
    layout.part_count = UI_NP_PARTS_MAX;
    for (uint8_t i = 0; i < UI_NP_PARTS_MAX; i++) {
        const int16_t x = (int16_t)(layout.text_x + i * part_w);
        const int16_t zone_w = i + 1 < UI_NP_PARTS_MAX ? part_w : (int16_t)(layout.text_x + layout.text_w - x);
        layout.parts[i] = k_row[i];
        layout.zones[i] = (ui_rect_t){ x, y, zone_w, UI_NP_ROW_H };
    }
    return layout;
}

ui_rect_t ui_np_cover_crop(int16_t w, int16_t h, ui_rect_t art)
{
    const int16_t crop_w = w < art.w ? w : art.w;
    const int16_t crop_h = h < art.h ? h : art.h;
    return (ui_rect_t){ (int16_t)((w - crop_w) / 2), (int16_t)((h - crop_h) / 2), crop_w, crop_h };
}

ui_part_t ui_np_hit(const ui_np_layout_t *layout, int16_t x, int16_t y, ui_np_state_t state)
{
    if (state == UI_NP_IDLE) {
        return UI_PART_NONE;
    }
    for (uint8_t i = 0; i < layout->part_count; i++) {
        const ui_rect_t r = layout->zones[i];
        if (x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h) {
            return layout->parts[i];
        }
    }
    return UI_PART_NONE;
}
