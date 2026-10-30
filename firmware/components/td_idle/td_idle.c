#include "td_idle.h"
#include <stdio.h>
#include <string.h>

#define US_PER_S INT64_C(1000000)
#define US_PER_MS INT64_C(1000)
#define MIN_PER_DAY 1440

void td_idle_defaults(td_idle_settings_t *out)
{
    *out = (td_idle_settings_t){
        .enabled = false,
        .timeout_s = TD_IDLE_TIMEOUT_DEFAULT_S,
        .interval_s = TD_IDLE_INTERVAL_DEFAULT_S,
    };
}

bool td_idle_valid(const td_idle_settings_t *settings)
{
    return settings->timeout_s >= TD_IDLE_TIMEOUT_MIN_S && settings->timeout_s <= TD_IDLE_TIMEOUT_MAX_S
           && settings->interval_s >= TD_IDLE_INTERVAL_MIN_S && settings->interval_s <= TD_IDLE_INTERVAL_MAX_S;
}

td_photo_admit_t td_photo_admit(const td_photo_pace_t *pace, uint32_t bytes, uint32_t limit, bool idle, uint32_t entry,
                                uint32_t interval_s, int64_t now_us, uint32_t *retry_ms)
{
    if (bytes == 0 || bytes > limit || bytes > TD_PHOTO_BYTES_MAX) {
        return TD_PHOTO_SIZE;
    }
    if (!idle) {
        return TD_PHOTO_IDLE;
    }
    if (!pace->shown || pace->entry != entry) {
        return TD_PHOTO_OK;
    }
    const int64_t due_us = pace->last_us + (int64_t)interval_s * US_PER_S;
    if (now_us >= due_us) {
        return TD_PHOTO_OK;
    }
    const int64_t wait_ms = (due_us - now_us + US_PER_MS - 1) / US_PER_MS;
    *retry_ms = wait_ms < 1 ? 1 : (uint32_t)wait_ms;
    return TD_PHOTO_RATE;
}

void td_photo_shown(td_photo_pace_t *pace, uint32_t entry, int64_t now_us)
{
    pace->entry = entry;
    pace->shown = true;
    pace->last_us = now_us;
}

td_idle_face_t td_idle_face(bool online, bool has_picture, int64_t picture_us, uint32_t interval_s, int64_t now_us)
{
    if (!online || !has_picture) {
        return TD_FACE_CLOCK;
    }
    return now_us - picture_us >= 2 * (int64_t)interval_s * US_PER_S ? TD_FACE_CLOCK : TD_FACE_PICTURE;
}

bool td_clock_valid(uint64_t unix_s, int32_t offset_min)
{
    return unix_s <= TD_CLOCK_UNIX_MAX_S && offset_min >= -TD_CLOCK_OFFSET_MAX_MIN
           && offset_min <= TD_CLOCK_OFFSET_MAX_MIN;
}

void td_clock_set(td_clock_t *clock, uint64_t unix_s, int32_t offset_min, int64_t now_us)
{
    *clock = (td_clock_t){ .synced = true, .unix_s = unix_s, .offset_min = offset_min, .at_us = now_us };
}

bool td_clock_text(const td_clock_t *clock, int64_t now_us, char out[6])
{
    if (!clock->synced) {
        memcpy(out, "--:--", 6);
        return false;
    }
    const int64_t elapsed_s = now_us > clock->at_us ? (now_us - clock->at_us) / US_PER_S : 0;
    const int64_t local_min = ((int64_t)clock->unix_s + elapsed_s) / 60 + clock->offset_min;
    const int64_t of_day = ((local_min % MIN_PER_DAY) + MIN_PER_DAY) % MIN_PER_DAY;
    snprintf(out, 6, "%02d:%02d", (int)(of_day / 60), (int)(of_day % 60));
    return true;
}
