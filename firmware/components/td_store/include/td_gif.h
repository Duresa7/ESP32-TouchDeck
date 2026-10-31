#pragma once
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define TD_GIF_MAX_BYTES 262144u
#define TD_GIF_MAX_SIDE 128u
#define TD_GIF_MAX_FRAMES 60u
#define TD_GIF_MAX_PIXELS 983040u
#define TD_GIF_MIN_DELAY_CS 10u
#define TD_GIF_SLICE_WORK 256u

typedef enum { TD_GIF_OK, TD_GIF_FORMAT, TD_GIF_SIZE, TD_GIF_MEMORY, TD_GIF_CANCELLED } td_gif_result_t;
typedef size_t (*td_gif_read_fn)(void *ctx, uint8_t *out, size_t bytes);
typedef bool (*td_gif_yield_fn)(void *ctx);
/* Reads exactly bytes through bounded reads, validates all LZW output, and yields
 * at most every TD_GIF_SLICE_WORK input bytes or codes. False cancels validation.
 * No frame buffers are allocated. The caller owns the stream and publication. */
td_gif_result_t td_gif_validate(td_gif_read_fn read, td_gif_yield_fn yield, void *ctx, uint32_t bytes);
