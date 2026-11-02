#include "td_gif.h"
#include <stdlib.h>
#include <string.h>
#ifdef ESP_PLATFORM
#include "esp_heap_caps.h"
#endif

typedef struct {
    td_gif_read_fn read;
    td_gif_yield_fn yield;
    void *ctx;
    uint32_t remaining;
    unsigned work;
    td_gif_result_t result;
    uint16_t lengths[4096];
    uint8_t largest[4096];
    uint8_t first[4096];
    td_gif_t *decoder;
} validator_t;

struct td_gif {
    validator_t validator;
    td_gif_info_t info;
    const uint8_t *data;
    uint32_t bytes, at;
    td_gif_yield_fn yield;
    void *ctx;
    uint32_t offsets[TD_GIF_MAX_FRAMES];
    int16_t transparent[TD_GIF_MAX_FRAMES];
    uint16_t palette[256];
    uint16_t prefix[4096];
    uint8_t suffix[4096], stack[4096];
    uint8_t transparent_mask[TD_GIF_MAX_SIDE * TD_GIF_MAX_SIDE / 8];
    uint8_t *pixels;
    unsigned colors, frame, pixel_count;
    bool indexing, independent;
};

static bool work(validator_t *v)
{
    if (v->result != TD_GIF_OK) {
        return false;
    }
    if (++v->work == TD_GIF_SLICE_WORK) {
        v->work = 0;
        if (!v->yield(v->ctx)) {
            v->result = TD_GIF_CANCELLED;
        }
    }
    return v->result == TD_GIF_OK;
}

static int byte(validator_t *v)
{
    if (!work(v)) {
        return -1;
    }
    uint8_t b;
    if (!v->remaining || v->read(v->ctx, &b, 1) != 1) {
        v->result = TD_GIF_FORMAT;
        return -1;
    }
    v->remaining--;
    return b;
}

static bool take(validator_t *v, uint8_t *out, size_t n)
{
    for (size_t i = 0; i < n; i++) {
        int b = byte(v);
        if (b < 0) {
            return false;
        }
        out[i] = (uint8_t)b;
    }
    return true;
}

static unsigned le16(const uint8_t *p) { return p[0] | ((unsigned)p[1] << 8); }

static bool emit(validator_t *v, unsigned position, uint8_t index)
{
    td_gif_t *d = v->decoder;
    if (!work(v)) return false;
    const bool transparent = index == d->transparent[d->frame];
    if (d->indexing) {
        const uint8_t bit = (uint8_t)(1u << (position % 8));
        if (d->frame == 0 && transparent) d->transparent_mask[position / 8] |= bit;
        if (d->frame && transparent != ((d->transparent_mask[position / 8] & bit) != 0)) {
            d->independent = false;
        }
    } else if (!transparent) {
        const uint16_t color = d->palette[index];
        d->pixels[position * 2] = (uint8_t)color;
        d->pixels[position * 2 + 1] = (uint8_t)(color >> 8);
        d->pixels[d->pixel_count * 2 + position] = 255;
    }
    return true;
}

static bool expand(validator_t *v, unsigned code, unsigned next, unsigned previous,
                   unsigned clear, unsigned first, unsigned position)
{
    td_gif_t *d = v->decoder;
    unsigned n = 0;
    if (code == next) {
        d->stack[n++] = (uint8_t)first;
        code = previous;
    }
    while (code >= clear) {
        if (!work(v) || code >= next || n >= sizeof d->stack) return false;
        d->stack[n++] = d->suffix[code];
        code = d->prefix[code];
    }
    if (n >= sizeof d->stack) return false;
    d->stack[n++] = (uint8_t)code;
    while (n) {
        if (!emit(v, position++, d->stack[--n])) return false;
    }
    return true;
}

static bool lzw(validator_t *v, unsigned pixels, unsigned colors)
{
    int minimum = byte(v);
    if (minimum < 2 || minimum > 8) {
        return false;
    }
    unsigned clear = 1u << minimum, end = clear + 1;
    unsigned next = end + 1, width = (unsigned)minimum + 1;
    unsigned previous = 4096, count = 0, block = 0, bits = 0;
    uint32_t packed = 0;
    bool started = false;
    for (unsigned i = 0; i < clear; i++) {
        v->lengths[i] = 1;
        v->largest[i] = v->first[i] = (uint8_t)i;
    }
    for (;;) {
        if (!work(v)) {
            return false;
        }
        while (bits < width) {
            if (!block) {
                int n = byte(v);
                if (n <= 0) {
                    return false;
                }
                block = (unsigned)n;
            }
            int b = byte(v);
            if (b < 0) {
                return false;
            }
            block--;
            packed |= (uint32_t)b << bits;
            bits += 8;
        }
        unsigned code = packed & ((1u << width) - 1);
        packed >>= width;
        bits -= width;
        if (!started && code != clear) {
            return false;
        }
        started = true;
        if (code == clear) {
            next = end + 1;
            width = (unsigned)minimum + 1;
            previous = 4096;
            continue;
        }
        if (code == end) {
            return count == pixels && block == 0 && byte(v) == 0;
        }
        if (code > next || code >= 4096 || (code == next && previous == 4096)) {
            return false;
        }
        unsigned length = code == next ? (unsigned)v->lengths[previous] + 1 : v->lengths[code];
        unsigned first = code == next ? v->first[previous] : v->first[code];
        unsigned largest = code == next ? v->largest[previous] : v->largest[code];
        if (largest >= colors || length > pixels - count) {
            return false;
        }
        if (v->decoder && !expand(v, code, next, previous, clear, first, count)) return false;
        count += length;
        if (previous != 4096 && next < 4096) {
            if (v->decoder) {
                v->decoder->prefix[next] = (uint16_t)previous;
                v->decoder->suffix[next] = (uint8_t)first;
            }
            v->lengths[next] = (uint16_t)(v->lengths[previous] + 1);
            v->first[next] = v->first[previous];
            v->largest[next] = (uint8_t)(v->largest[previous] > first ? v->largest[previous] : first);
            next++;
            if (next == (1u << width) && width < 12) {
                width++;
            }
        }
        previous = code;
    }
}

static td_gif_result_t validate(validator_t *v)
{
    uint8_t h[13];
    if (!take(v, h, sizeof h) || memcmp(h, "GIF89a", 6)) {
        return TD_GIF_FORMAT;
    }
    unsigned w = le16(h + 6), height = le16(h + 8);
    if (!w || !height || w > TD_GIF_MAX_SIDE || height > TD_GIF_MAX_SIDE) {
        return TD_GIF_SIZE;
    }
    unsigned colors = 2u << (h[10] & 7);
    if (!(h[10] & 128) || h[11] >= colors || h[12]) {
        return TD_GIF_FORMAT;
    }
    td_gif_t *d = v->decoder;
    if (d) {
        d->info.width = (uint16_t)w;
        d->info.height = (uint16_t)height;
        d->colors = colors;
        d->pixel_count = w * height;
    }
    for (unsigned i = 0; i < colors; i++) {
        uint8_t rgb[3];
        if (!take(v, rgb, sizeof rgb)) {
            return TD_GIF_FORMAT;
        }
        if (d) d->palette[i] = (uint16_t)(((rgb[0] & 0xf8) << 8) | ((rgb[1] & 0xfc) << 3) | (rgb[2] >> 3));
    }
    bool loop = false, control = false;
    int transparent = -1;
    unsigned frames = 0, pixels = w * height;
    unsigned delay_ms = 0;
    int frame_transparent = -1;
    for (;;) {
        int marker = byte(v);
        if (marker == 0x3b) {
            return frames && loop && !control && !v->remaining ? TD_GIF_OK : TD_GIF_FORMAT;
        }
        if (marker == 0x21) {
            int extension = byte(v);
            if (extension == 0xff) {
                uint8_t app[17];
                static const uint8_t netscape[] = {11,  'N', 'E', 'T', 'S', 'C', 'A', 'P', 'E',
                                                   '2', '.', '0', 3,   1,   0,   0,   0};
                if (loop || frames || control || !take(v, app, sizeof app) || memcmp(app, netscape, sizeof app)) {
                    return TD_GIF_FORMAT;
                }
                loop = true;
            } else if (extension == 0xf9) {
                uint8_t gce[6];
                if (control || !take(v, gce, sizeof gce) || gce[0] != 4 || gce[5] || (gce[1] & 0xfa) ||
                    le16(gce + 2) < TD_GIF_MIN_DELAY_CS) {
                    return TD_GIF_FORMAT;
                }
                if (gce[1] & 1) {
                    if (gce[4] >= colors || (transparent >= 0 && transparent != gce[4])) {
                        return TD_GIF_FORMAT;
                    }
                    transparent = gce[4];
                }
                frame_transparent = gce[1] & 1 ? gce[4] : -1;
                delay_ms = le16(gce + 2) * 10;
                control = true;
            } else {
                return TD_GIF_FORMAT;
            }
        } else if (marker == 0x2c) {
            uint8_t rect[9];
            if (!control || !loop || !take(v, rect, sizeof rect)) {
                return TD_GIF_FORMAT;
            }
            if (le16(rect) || le16(rect + 2) || le16(rect + 4) != w || le16(rect + 6) != height || rect[8]) {
                return TD_GIF_FORMAT;
            }
            if (++frames > TD_GIF_MAX_FRAMES || frames > TD_GIF_MAX_PIXELS / pixels) {
                return TD_GIF_SIZE;
            }
            if (d) {
                d->frame = frames - 1;
                d->offsets[d->frame] = d->at;
                d->transparent[d->frame] = (int16_t)frame_transparent;
                d->info.delays_ms[d->frame] = delay_ms;
                d->info.duration_ms += delay_ms;
                d->info.frames = (uint16_t)frames;
            }
            if (!lzw(v, pixels, colors)) {
                return TD_GIF_FORMAT;
            }
            control = false;
        } else {
            return TD_GIF_FORMAT;
        }
    }
}

td_gif_result_t td_gif_validate(td_gif_read_fn read, td_gif_yield_fn yield, void *ctx, uint32_t bytes)
{
    if (bytes > TD_GIF_MAX_BYTES) {
        return TD_GIF_SIZE;
    }
    if (!read || !yield) {
        return TD_GIF_FORMAT;
    }
    validator_t *v = calloc(1, sizeof *v);
    if (!v) {
        return TD_GIF_MEMORY;
    }
    v->read = read;
    v->yield = yield;
    v->ctx = ctx;
    v->remaining = bytes;
    td_gif_result_t result = validate(v);
    if (v->result != TD_GIF_OK) {
        result = v->result;
    }
    free(v);
    return result;
}

static size_t memory_read(void *ctx, uint8_t *out, size_t bytes)
{
    td_gif_t *d = ctx;
    if (bytes > d->bytes - d->at) return 0;
    memcpy(out, d->data + d->at, bytes);
    d->at += (uint32_t)bytes;
    return bytes;
}

static bool memory_yield(void *ctx)
{
    td_gif_t *d = ctx;
    return d->yield(d->ctx);
}

td_gif_result_t td_gif_open(const uint8_t *data, uint32_t bytes, td_gif_yield_fn yield, void *ctx, td_gif_t **out)
{
    if (!out) return TD_GIF_FORMAT;
    *out = NULL;
    if (bytes > TD_GIF_MAX_BYTES) return TD_GIF_SIZE;
    if (!data || !yield) return TD_GIF_FORMAT;
#ifdef ESP_PLATFORM
    td_gif_t *d = heap_caps_calloc(1, sizeof *d, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
#else
    td_gif_t *d = calloc(1, sizeof *d);
#endif
    if (!d) return TD_GIF_MEMORY;
    d->data = data;
    d->bytes = bytes;
    d->yield = yield;
    d->ctx = ctx;
    d->indexing = d->independent = true;
    validator_t *v = &d->validator;
    v->read = memory_read;
    v->yield = memory_yield;
    v->ctx = d;
    v->remaining = bytes;
    v->decoder = d;
    td_gif_result_t result = validate(v);
    if (v->result != TD_GIF_OK) result = v->result;
    d->indexing = false;
    if (result == TD_GIF_OK) *out = d;
    else td_gif_close(d);
    return result;
}

const td_gif_info_t *td_gif_info(const td_gif_t *gif) { return &gif->info; }

td_gif_result_t td_gif_render(td_gif_t *d, uint32_t frame, uint8_t *pixels, size_t bytes,
                             td_gif_yield_fn yield, void *ctx)
{
    if (!d || !pixels || !yield || frame >= d->info.frames) return TD_GIF_FORMAT;
    const size_t needed = d->pixel_count * 3u;
    if (bytes < needed) return TD_GIF_SIZE;
    d->yield = yield;
    d->ctx = ctx;
    d->pixels = pixels;
    validator_t *v = &d->validator;
    v->result = TD_GIF_OK;
    v->work = 0;
    for (size_t i = 0; i < needed; i++) {
        if (!work(v)) return v->result;
        pixels[i] = 0;
    }
    for (unsigned f = d->independent ? frame : 0; f <= frame; f++) {
        d->frame = f;
        d->at = d->offsets[f];
        v->remaining = d->bytes - d->at;
        if (!lzw(v, d->pixel_count, d->colors)) {
            return v->result == TD_GIF_OK ? TD_GIF_FORMAT : v->result;
        }
    }
    return TD_GIF_OK;
}

void td_gif_close(td_gif_t *gif) { free(gif); }
