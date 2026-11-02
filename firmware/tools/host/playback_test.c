#include "ui_gif_player.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const uint8_t header[] = {
    'G','I','F','8','9','a', 1,0,1,0,0x80,0,0, 0,0,0,255,255,255,
    0x21,0xff,11,'N','E','T','S','C','A','P','E','2','.','0',3,1,0,0,0
};
static const uint8_t frame[] = {0x21,0xf9,4,4,10,0,0,0,0x2c,0,0,0,0,1,0,1,0,0,2,2,0x44,1,0};
typedef struct {
    uint64_t now, slow;
    unsigned allocations, frees, shown, fail_at, yields, cancel_at;
    const uint8_t *front;
    bool cancel;
} harness_t;
static void *allocate(void *ctx, size_t bytes)
{
    harness_t *h = ctx;
    if (h->fail_at && h->allocations + 1 == h->fail_at) return NULL;
    h->allocations++;
    return malloc(bytes);
}
static void release(void *ctx, void *ptr)
{
    harness_t *h = ctx;
    if (ptr) { assert(ptr != h->front); h->frees++; free(ptr); }
}
static uint64_t now(void *ctx) { return ((harness_t *)ctx)->now; }
static bool yield(void *ctx)
{
    harness_t *h = ctx;
    h->now += h->slow;
    h->yields++;
    return !h->cancel && (!h->cancel_at || h->yields < h->cancel_at);
}
static bool in_use(void *ctx, const uint8_t *pixels) { return pixels && pixels == ((harness_t *)ctx)->front; }
static bool present(void *ctx, const uint8_t *pixels, const td_gif_info_t *info, uint64_t deadline)
{
    harness_t *h = ctx;
    assert(info->width && info->height);
    assert(!deadline || h->now < deadline);
    h->front = pixels;
    h->shown++;
    return true;
}
static ui_gif_player_t *open_player(harness_t *h, uint8_t **poster, uint8_t *data, uint32_t bytes)
{
    ui_gif_ops_t ops = {allocate, release, now, yield, in_use, present};
    ui_gif_player_t *p = NULL;
    assert(ui_gif_player_open(data, bytes, poster, &ops, h, &p) == TD_GIF_OK);
    return p;
}
int main(int argc, char **argv)
{
    uint8_t data[sizeof header + 3 * sizeof frame + 1];
    memcpy(data, header, sizeof header);
    for (unsigned i = 0; i < 3; i++) memcpy(data + sizeof header + i * sizeof frame, frame, sizeof frame);
    data[sizeof data - 1] = 0x3b;
    harness_t h = {0};
    uint8_t *poster = NULL;
    ui_gif_player_t *p = open_player(&h, &poster, data, sizeof data);
    assert(ui_gif_player_tick(p, true) == TD_GIF_OK);
    assert(h.front == poster && h.shown == 1);
    h.now = 99;
    ui_gif_player_tick(p, true);
    assert(h.shown == 1);
    h.now = 100;
    ui_gif_player_tick(p, true);
    assert(h.front != poster && h.shown == 2);
    h.now = 750;
    ui_gif_player_tick(p, true);
    assert(h.shown == 3);
    assert(ui_gif_player_skipped(p) >= 5);
    const unsigned shown = h.shown;
    h.now = 800;
    h.slow = 150;
    ui_gif_player_tick(p, true);
    assert(h.shown == shown);
    assert(ui_gif_player_skipped(p) >= 6);
    h.slow = 0;
    for (unsigned reason = 0; reason < 5; reason++) {
        ui_gif_visibility_t visibility = {true, false, false, false, false};
        if (reason == 0) visibility.on_page = false;
        if (reason == 1) visibility.sliding = true;
        if (reason == 2) visibility.idle = true;
        if (reason == 3) visibility.dimmed = true;
        if (reason == 4) visibility.off = true;
        assert(!ui_gif_should_play(visibility));
        ui_gif_player_tick(p, ui_gif_should_play(visibility));
        assert(h.front == poster);
        assert(h.allocations - h.frees == 2);
        ui_gif_player_tick(p, true);
        h.now += 100;
        ui_gif_player_tick(p, true);
        assert(h.front != poster);
    }
    h.cancel = true;
    h.now += 100;
    assert(ui_gif_player_tick(p, true) == TD_GIF_CANCELLED);
    h.cancel = false;
    ui_gif_player_tick(p, false);
    assert(h.front == poster && h.allocations - h.frees == 2);
    h.front = NULL;
    assert(ui_gif_player_close(p));
    release(&h, poster);
    assert(h.allocations == h.frees);

    for (unsigned allocation = 1; allocation <= 2; allocation++) {
        harness_t failure = {.fail_at = allocation};
        ui_gif_ops_t ops = {allocate, release, now, yield, in_use, present};
        poster = NULL;
        p = NULL;
        assert(ui_gif_player_open(data, sizeof data, &poster, &ops, &failure, &p) == TD_GIF_MEMORY);
        assert(!p && !poster && failure.allocations == failure.frees);
    }

    unsigned count = 0;
    uint32_t pixels = 0;
    for (unsigned i = 0; i < 4; i++) assert(ui_gif_budget_add(&count, &pixels, 128, 128, 4, 65536));
    assert(!ui_gif_budget_add(&count, &pixels, 1, 1, 4, 65536));
    assert(count == 4 && pixels == 65536);
    count = 0; pixels = 0;
    assert(!ui_gif_budget_add(&count, &pixels, 128, 128, 4, 16383));
    assert(count == 0 && pixels == 0);
    assert(argc == 2);
    FILE *fixture = fopen(argv[1], "rb");
    assert(fixture);
    uint8_t encoded[32768];
    const size_t bytes = fread(encoded, 1, sizeof encoded, fixture);
    assert(bytes && bytes < sizeof encoded && fclose(fixture) == 0);
    h = (harness_t){0};
    poster = NULL;
    p = open_player(&h, &poster, encoded, (uint32_t)bytes);
    ui_gif_player_tick(p, true);
    h.now = 100;
    h.cancel_at = h.yields + 5;
    assert(ui_gif_player_tick(p, true) == TD_GIF_CANCELLED);
    assert(h.yields == h.cancel_at && h.front == poster);
    h.cancel_at = 0;
    h.now = 200;
    h.slow = 20;
    unsigned before = h.shown;
    assert(ui_gif_player_tick(p, true) == TD_GIF_OK);
    assert(h.shown == before && h.front == poster);
    assert(ui_gif_player_skipped(p) > 0);
    h.slow = 0;
    h.front = NULL;
    assert(ui_gif_player_close(p));
    release(&h, poster);
    assert(h.allocations == h.frees);
    puts("playback timing, skipping, pause, cancellation, ownership and Page budgets: OK");
    return 0;
}
