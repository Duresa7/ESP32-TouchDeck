#pragma once
#include "td_gif.h"

typedef struct ui_gif_player ui_gif_player_t;
typedef struct {
    bool on_page, sliding, idle, dimmed, off;
} ui_gif_visibility_t;

typedef struct {
    void *(*allocate)(void *ctx, size_t bytes);
    void (*release)(void *ctx, void *ptr);
    uint64_t (*now_ms)(void *ctx);
    bool (*yield)(void *ctx);
    bool (*in_use)(void *ctx, const uint8_t *pixels);
    bool (*present)(void *ctx, const uint8_t *pixels, const td_gif_info_t *info, uint64_t deadline_ms);
} ui_gif_ops_t;

/* All player calls belong to one worker. Encoded bytes and the caller-owned poster
 * outlive the player. present/in_use hand buffers to a renderer without decoding
 * under its lock. A nonzero deadline expires the publication, including in transit. */
td_gif_result_t ui_gif_player_open(const uint8_t *data, uint32_t bytes, uint8_t **poster,
                                  const ui_gif_ops_t *ops, void *ctx, ui_gif_player_t **out);
td_gif_result_t ui_gif_player_tick(ui_gif_player_t *player, bool playing);
/* False until the renderer has released both playback buffers. */
bool ui_gif_player_close(ui_gif_player_t *player);
uint64_t ui_gif_player_skipped(const ui_gif_player_t *player);
const td_gif_info_t *ui_gif_player_info(const ui_gif_player_t *player);
bool ui_gif_should_play(ui_gif_visibility_t visibility);
bool ui_gif_budget_add(unsigned *tiles, uint32_t *pixels, uint32_t width, uint32_t height,
                       unsigned max_tiles, uint32_t max_pixels);
