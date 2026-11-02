#include "ui_gif_player.h"
#include <string.h>

struct ui_gif_player {
    td_gif_t *gif;
    ui_gif_ops_t ops;
    void *ctx;
    uint8_t *poster, *buffers[2];
    size_t bytes;
    uint64_t epoch, attempted, skipped, deadline;
    bool started;
};

bool ui_gif_should_play(ui_gif_visibility_t v)
{
    return v.on_page && !v.sliding && !v.idle && !v.dimmed && !v.off;
}

bool ui_gif_budget_add(unsigned *tiles, uint32_t *pixels, uint32_t width, uint32_t height,
                       unsigned max_tiles, uint32_t max_pixels)
{
    if (*tiles >= max_tiles || !width || !height || *pixels > max_pixels ||
        width > (max_pixels - *pixels) / height) return false;
    ++*tiles;
    *pixels += width * height;
    return true;
}

td_gif_result_t ui_gif_player_open(const uint8_t *data, uint32_t bytes, uint8_t **poster,
                                  const ui_gif_ops_t *ops, void *ctx, ui_gif_player_t **out)
{
    *out = NULL;
    ui_gif_player_t *p = ops->allocate(ctx, sizeof *p);
    if (!p) return TD_GIF_MEMORY;
    memset(p, 0, sizeof *p);
    p->ops = *ops;
    p->ctx = ctx;
    td_gif_result_t result = td_gif_open(data, bytes, ops->yield, ctx, &p->gif);
    if (result == TD_GIF_OK) {
        const td_gif_info_t *info = td_gif_info(p->gif);
        p->bytes = (size_t)info->width * info->height * 3;
        p->poster = *poster;
        if (!p->poster) {
            p->poster = ops->allocate(ctx, p->bytes);
            result = p->poster ? td_gif_render(p->gif, 0, p->poster, p->bytes, ops->yield, ctx) : TD_GIF_MEMORY;
        }
    }
    if (result != TD_GIF_OK) {
        if (p->poster != *poster) ops->release(ctx, p->poster);
        td_gif_close(p->gif);
        ops->release(ctx, p);
        return result;
    }
    *poster = p->poster;
    *out = p;
    return TD_GIF_OK;
}

static void free_unused(ui_gif_player_t *p)
{
    for (unsigned i = 0; i < 2; i++) {
        if (p->buffers[i] && !p->ops.in_use(p->ctx, p->buffers[i])) {
            p->ops.release(p->ctx, p->buffers[i]);
            p->buffers[i] = NULL;
        }
    }
}

static bool frame_yield(void *ctx)
{
    ui_gif_player_t *p = ctx;
    return p->ops.yield(p->ctx) && p->ops.now_ms(p->ctx) < p->deadline;
}

td_gif_result_t ui_gif_player_tick(ui_gif_player_t *p, bool playing)
{
    const td_gif_info_t *info = td_gif_info(p->gif);
    if (!playing) {
        p->started = false;
        p->ops.present(p->ctx, p->poster, info, 0);
        free_unused(p);
        return TD_GIF_OK;
    }
    const uint64_t now = p->ops.now_ms(p->ctx);
    if (!p->started) {
        if (!p->ops.present(p->ctx, p->poster, info, 0)) return TD_GIF_OK;
        p->epoch = now;
        p->attempted = 0;
        p->started = true;
        return TD_GIF_OK;
    }
    const uint64_t elapsed = now - p->epoch;
    const uint64_t cycle = elapsed / info->duration_ms;
    uint32_t within = (uint32_t)(elapsed % info->duration_ms);
    unsigned frame = 0;
    uint32_t end = info->delays_ms[0];
    while (within >= end && frame + 1 < info->frames) end += info->delays_ms[++frame];
    const uint64_t cursor = cycle * info->frames + frame;
    if (cursor == p->attempted) return TD_GIF_OK;
    const uint64_t deadline = p->epoch + cycle * info->duration_ms + end;
    if (!frame) {
        if (p->ops.present(p->ctx, p->poster, info, deadline)) {
            if (cursor > p->attempted + 1) p->skipped += cursor - p->attempted - 1;
            p->attempted = cursor;
        }
        return TD_GIF_OK;
    }
    uint8_t *back = NULL;
    for (unsigned i = 0; i < 2; i++) {
        if (!p->buffers[i]) p->buffers[i] = p->ops.allocate(p->ctx, p->bytes);
        if (!p->buffers[i]) return TD_GIF_MEMORY;
        if (!p->ops.in_use(p->ctx, p->buffers[i])) { back = p->buffers[i]; break; }
    }
    if (!back) return TD_GIF_OK;
    if (cursor > p->attempted + 1) p->skipped += cursor - p->attempted - 1;
    p->attempted = cursor;
    p->deadline = deadline;
    td_gif_result_t result = frame_yield(p) ?
        td_gif_render(p->gif, frame, back, p->bytes, frame_yield, p) : TD_GIF_CANCELLED;
    if (result == TD_GIF_CANCELLED && p->ops.now_ms(p->ctx) >= deadline) {
        p->skipped++;
        return TD_GIF_OK;
    }
    if (result != TD_GIF_OK) return result;
    if (p->ops.now_ms(p->ctx) >= deadline || !p->ops.present(p->ctx, back, info, deadline)) p->skipped++;
    return TD_GIF_OK;
}

bool ui_gif_player_close(ui_gif_player_t *p)
{
    if (!p) return true;
    free_unused(p);
    if (p->buffers[0] || p->buffers[1]) return false;
    td_gif_close(p->gif);
    p->ops.release(p->ctx, p);
    return true;
}

uint64_t ui_gif_player_skipped(const ui_gif_player_t *p) { return p->skipped; }
const td_gif_info_t *ui_gif_player_info(const ui_gif_player_t *p) { return td_gif_info(p->gif); }
