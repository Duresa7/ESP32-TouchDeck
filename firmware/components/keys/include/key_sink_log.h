#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "esp_err.h"
#include "freertos/FreeRTOS.h"
#include "key_sink.h"
#include "sdkconfig.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Reports the record holds once full. The 2049th and later are counted in dropped. */
#define KEY_LOG_RECORD_CAP 2048

/* Room for the longest JSON of key_sink_log_format, which is 90 characters, plus the terminator. */
#define KEY_LOG_LINE_MAX 128

typedef struct {
    uint32_t seq;
    uint32_t t;
    key_report_t report;
} key_log_entry_t;

/*
 * The console key log sink. Each report gets seq, counted from 1 since boot, and t, milliseconds since boot.
 * With the log on it prints one line per report. With recording on it appends the report to the record until
 * the record is full, and counts the rest in dropped.
 *
 * report() may run on any task, under the key engine's lock, and the control calls may run on another, so the
 * fields are guarded by a spinlock held only for a few stores. The entries below count stay as they are until
 * the next key_sink_log_record_start, so a dump can read them without the lock.
 */
typedef struct key_sink_log {
    key_log_entry_t *entries;
    size_t cap;
    uint32_t (*now_ms)(void);
    void (*print)(const char *line);
    portMUX_TYPE lock;
    uint32_t seq;
    bool log_on;
    bool recording;
    size_t count;
    uint32_t dropped;
#if CONFIG_TD_LAZY_KEY_RECORD
    /* key_sink_log_start's record waits for the first key_sink_log_record_start to allocate it. */
    bool record_on_start;
#endif
} key_sink_log_t;

/*
 * entries holds cap reports, and may be NULL with cap 0 for a sink that records nothing. now_ms NULL means
 * esp_timer, and print NULL means an info log line with the tag keys, so the line reads `keys: {...}`.
 */
void key_sink_log_init(key_sink_log_t *l, key_log_entry_t *entries, size_t cap, uint32_t (*now_ms)(void),
                       void (*print)(const char *line));

/* key_sink_log_init with a record of KEY_LOG_RECORD_CAP entries in PSRAM, and the board clock and log. With no
 * PSRAM to spare it keeps a record of 0 and returns ESP_ERR_NO_MEM, and the sink still logs. With
 * CONFIG_TD_LAZY_KEY_RECORD the record is allocated by the first key_sink_log_record_start instead, and kept. */
esp_err_t key_sink_log_start(key_sink_log_t *l);

key_sink_t key_sink_log_sink(key_sink_log_t *l);

/* The JSON of one entry, without a prefix, as `{"seq":N,"t":T,"type":"kbd","mods":M,"keys":[...]}` or
 * `{"seq":N,"t":T,"type":"con","usage":U}`. keys lists the nonzero slots in order. Returns the length. */
size_t key_sink_log_format(const key_log_entry_t *e, char *buf, size_t cap);

void key_sink_log_set_log(key_sink_log_t *l, bool on);
bool key_sink_log_log_on(const key_sink_log_t *l);
uint32_t key_sink_log_seq(const key_sink_log_t *l);

/* Empties the record and starts it. */
void key_sink_log_record_start(key_sink_log_t *l);
void key_sink_log_record_stop(key_sink_log_t *l);
bool key_sink_log_recording(const key_sink_log_t *l);
size_t key_sink_log_count(const key_sink_log_t *l);
uint32_t key_sink_log_dropped(const key_sink_log_t *l);

/* Entry i of the record, for i below key_sink_log_count. */
const key_log_entry_t *key_sink_log_entry(const key_sink_log_t *l, size_t i);

#ifdef __cplusplus
}
#endif
