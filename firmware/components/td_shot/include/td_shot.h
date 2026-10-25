#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    TD_SHOT_SCALE_1 = 1,
    TD_SHOT_SCALE_2 = 2,
    TD_SHOT_SCALE_4 = 4,
} td_shot_scale_t;

/* RGB565, 2 bytes per pixel in memory order (low byte first), row-major, stride in bytes. */
typedef struct {
    const uint8_t *px;
    uint16_t width, height;
    uint32_t stride;
} td_shot_image_t;

/* Geometry of the output image. */
typedef struct {
    uint16_t width, height;
    uint32_t bytes, lines;
} td_shot_layout_t;

#define TD_SHOT_LINE_PREFIX "@tdb "
/* A multiple of 3, so every full line is complete base64 with no '='. */
#define TD_SHOT_LINE_RAW 768
/* Prefix, base64 and the trailing newline. */
#define TD_SHOT_LINE_MAX (sizeof(TD_SHOT_LINE_PREFIX) - 1 + (TD_SHOT_LINE_RAW / 3) * 4 + 1)

/* One call is one whole line including its trailing '\n'. Return false to abort the stream. */
typedef bool (*td_shot_sink_t)(void *ctx, const char *line, size_t len);

/* NULL means scale 1. Only the strings "1", "2" and "4" are valid. */
bool td_shot_parse_scale(const char *arg, td_shot_scale_t *out);

/* All zero when src is NULL, the scale is not 1, 2 or 4, or a side is not divisible by the scale. */
td_shot_layout_t td_shot_layout(const td_shot_image_t *src, td_shot_scale_t scale);

/* Keeps the top-left pixel of each scale x scale block, then chunks, base64-encodes and CRC-32s the result.
 * *crc32 is the CRC-32 of the raw decimated bytes in stream order, equal to Python's zlib.crc32 over them.
 * It covers the lines the sink accepted, so after ESP_FAIL it covers the lines emitted before the rejected one.
 * ESP_ERR_INVALID_ARG, with no sink call and *crc32 untouched, for a NULL argument (ctx may be NULL), a bad scale,
 * a side not divisible by the scale, a stride below width * 2, or an output over 4 GiB. */
esp_err_t td_shot_stream(const td_shot_image_t *src, td_shot_scale_t scale, td_shot_sink_t sink, void *ctx,
                         uint32_t *crc32);

#ifdef __cplusplus
}
#endif
