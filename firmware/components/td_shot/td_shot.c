#include "td_shot.h"
#include <string.h>
#include "esp_rom_crc.h"

_Static_assert(TD_SHOT_LINE_RAW % 6 == 0, "a line holds whole pixels and whole base64 groups");

static const char k_b64[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

static bool scale_ok(td_shot_scale_t scale)
{
    return scale == TD_SHOT_SCALE_1 || scale == TD_SHOT_SCALE_2 || scale == TD_SHOT_SCALE_4;
}

static bool geometry_ok(const td_shot_image_t *src, td_shot_scale_t scale)
{
    if (src == NULL || !scale_ok(scale)) {
        return false;
    }
    const unsigned step = (unsigned)scale;
    if (src->width % step != 0 || src->height % step != 0) {
        return false;
    }
    return (uint64_t)(src->width / step) * (src->height / step) * 2 <= UINT32_MAX;
}

bool td_shot_parse_scale(const char *arg, td_shot_scale_t *out)
{
    if (out == NULL) {
        return false;
    }
    if (arg == NULL) {
        *out = TD_SHOT_SCALE_1;
        return true;
    }
    if (strcmp(arg, "1") == 0) {
        *out = TD_SHOT_SCALE_1;
    } else if (strcmp(arg, "2") == 0) {
        *out = TD_SHOT_SCALE_2;
    } else if (strcmp(arg, "4") == 0) {
        *out = TD_SHOT_SCALE_4;
    } else {
        return false;
    }
    return true;
}

td_shot_layout_t td_shot_layout(const td_shot_image_t *src, td_shot_scale_t scale)
{
    td_shot_layout_t layout = {0};
    if (!geometry_ok(src, scale)) {
        return layout;
    }
    const unsigned step = (unsigned)scale;
    layout.width = (uint16_t)(src->width / step);
    layout.height = (uint16_t)(src->height / step);
    layout.bytes = (uint32_t)layout.width * layout.height * 2;
    layout.lines = (layout.bytes + TD_SHOT_LINE_RAW - 1) / TD_SHOT_LINE_RAW;
    return layout;
}

static size_t encode_line(char *out, const uint8_t *in, size_t n)
{
    char *p = out;
    memcpy(p, TD_SHOT_LINE_PREFIX, sizeof(TD_SHOT_LINE_PREFIX) - 1);
    p += sizeof(TD_SHOT_LINE_PREFIX) - 1;

    size_t i = 0;
    for (; i + 3 <= n; i += 3) {
        const uint32_t v = (uint32_t)in[i] << 16 | (uint32_t)in[i + 1] << 8 | in[i + 2];
        *p++ = k_b64[v >> 18];
        *p++ = k_b64[(v >> 12) & 63];
        *p++ = k_b64[(v >> 6) & 63];
        *p++ = k_b64[v & 63];
    }
    if (n - i == 1) {
        const uint32_t v = (uint32_t)in[i] << 16;
        *p++ = k_b64[v >> 18];
        *p++ = k_b64[(v >> 12) & 63];
        *p++ = '=';
        *p++ = '=';
    } else if (n - i == 2) {
        const uint32_t v = (uint32_t)in[i] << 16 | (uint32_t)in[i + 1] << 8;
        *p++ = k_b64[v >> 18];
        *p++ = k_b64[(v >> 12) & 63];
        *p++ = k_b64[(v >> 6) & 63];
        *p++ = '=';
    }
    *p++ = '\n';
    return (size_t)(p - out);
}

static esp_err_t emit(const uint8_t *raw, size_t n, char *line, td_shot_sink_t sink, void *ctx, uint32_t *crc)
{
    const size_t len = encode_line(line, raw, n);
    if (!sink(ctx, line, len)) {
        return ESP_FAIL;
    }
    *crc = esp_rom_crc32_le(*crc, raw, (uint32_t)n);
    return ESP_OK;
}

esp_err_t td_shot_stream(const td_shot_image_t *src, td_shot_scale_t scale, td_shot_sink_t sink, void *ctx,
                         uint32_t *crc32)
{
    if (sink == NULL || crc32 == NULL || !geometry_ok(src, scale) || src->px == NULL ||
        src->stride < (uint32_t)src->width * 2) {
        return ESP_ERR_INVALID_ARG;
    }

    const unsigned step = (unsigned)scale;
    const unsigned out_w = src->width / step;
    const unsigned out_h = src->height / step;
    uint8_t raw[TD_SHOT_LINE_RAW];
    char line[TD_SHOT_LINE_MAX];
    size_t fill = 0;
    uint32_t crc = 0;
    esp_err_t err = ESP_OK;

    for (unsigned y = 0; y < out_h && err == ESP_OK; y++) {
        const uint8_t *row = src->px + (size_t)y * step * src->stride;
        for (unsigned x = 0; x < out_w && err == ESP_OK; x++) {
            const uint8_t *px = row + (size_t)x * step * 2;
            raw[fill++] = px[0];
            raw[fill++] = px[1];
            if (fill == sizeof raw) {
                err = emit(raw, fill, line, sink, ctx, &crc);
                fill = 0;
            }
        }
    }
    if (err == ESP_OK && fill > 0) {
        err = emit(raw, fill, line, sink, ctx, &crc);
    }

    *crc32 = crc;
    return err;
}
