#pragma once

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * The rules of the photo frame and the idle clock, from schema/protocol/link-v2.md "Photo frame, idle, and clock".
 * Nothing here keeps state of its own or reads a clock: the callers own every struct and pass the time in, so the rules
 * test without a Device.
 */

#define TD_IDLE_TIMEOUT_MIN_S 10
#define TD_IDLE_TIMEOUT_MAX_S 86400
#define TD_IDLE_INTERVAL_MIN_S 5
#define TD_IDLE_INTERVAL_MAX_S 3600
#define TD_IDLE_TIMEOUT_DEFAULT_S 60
#define TD_IDLE_INTERVAL_DEFAULT_S 15

/* The largest picture the protocol allows, which this Firmware also advertises as `photo_bytes`. */
#define TD_PHOTO_BYTES_MAX 131072u

#define TD_CLOCK_UNIX_MAX_S 4102444799ull
#define TD_CLOCK_OFFSET_MAX_MIN 840

typedef struct {
    bool enabled;
    uint32_t timeout_s;
    uint32_t interval_s;
} td_idle_settings_t;

/* Off, 60 s and 15 s. */
void td_idle_defaults(td_idle_settings_t *out);

/* Both times in range. */
bool td_idle_valid(const td_idle_settings_t *settings);

typedef enum {
    TD_PHOTO_OK,
    TD_PHOTO_SIZE, /* 0 bytes, or more than the limit */
    TD_PHOTO_IDLE, /* the photo frame is off or the Device is awake */
    TD_PHOTO_RATE, /* the interval since the last picture of this idle period has not passed */
} td_photo_admit_t;

/* When pictures may come. `entry` numbers the idle periods, so a new one is told from the last. */
typedef struct {
    uint32_t entry;
    bool shown;
    int64_t last_us;
} td_photo_pace_t;

/*
 * Whether a picture of `bytes` may start now. `idle` is whether the idle face shows with the photo frame on, and
 * `entry` which idle period it is. The first picture of a period may come at once; the next waits `interval_s` from
 * the last one shown, and RATE sets `retry_ms` to the rest of that wait, rounded up and at least 1.
 */
td_photo_admit_t td_photo_admit(const td_photo_pace_t *pace, uint32_t bytes, uint32_t limit, bool idle, uint32_t entry,
                                uint32_t interval_s, int64_t now_us, uint32_t *retry_ms);

/* A picture of idle period `entry` showed at now_us. */
void td_photo_shown(td_photo_pace_t *pace, uint32_t entry, int64_t now_us);

typedef enum {
    TD_FACE_CLOCK,
    TD_FACE_PICTURE,
} td_idle_face_t;

/* The idle face shows the picture while the link is up and the picture came in the last two intervals, and the clock
 * otherwise. `picture_us` is when the picture showed; `has_picture` is false before the first of the period. */
td_idle_face_t td_idle_face(bool online, bool has_picture, int64_t picture_us, uint32_t interval_s, int64_t now_us);

/* The Device's clock, which only the Companion sets and which counts on from the monotonic time of the set. */
typedef struct {
    bool synced;
    uint64_t unix_s;
    int32_t offset_min;
    int64_t at_us;
} td_clock_t;

/* unix_s 0 to 4102444799 and offset_min -840 to 840. */
bool td_clock_valid(uint64_t unix_s, int32_t offset_min);

void td_clock_set(td_clock_t *clock, uint64_t unix_s, int32_t offset_min, int64_t now_us);

/* The local time as "HH:MM", or "--:--" before the first set, which is when it returns false. */
bool td_clock_text(const td_clock_t *clock, int64_t now_us, char out[6]);

#ifdef __cplusplus
}
#endif
