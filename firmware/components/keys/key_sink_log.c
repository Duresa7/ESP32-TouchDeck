#include "key_sink_log.h"
#include <stdio.h>
#include <string.h>
#include "esp_heap_caps.h"
#include "esp_log.h"
#include "esp_timer.h"

static const char *TAG = "keys";

static uint32_t board_now_ms(void)
{
    return (uint32_t)(esp_timer_get_time() / 1000);
}

static void board_print(const char *line)
{
    ESP_LOGI(TAG, "%s", line);
}

void key_sink_log_init(key_sink_log_t *l, key_log_entry_t *entries, size_t cap, uint32_t (*now_ms)(void),
                       void (*print)(const char *line))
{
    memset(l, 0, sizeof *l);
    l->entries = entries;
    l->cap = entries ? cap : 0;
    l->now_ms = now_ms ? now_ms : board_now_ms;
    l->print = print ? print : board_print;
    portMUX_INITIALIZE(&l->lock);
}

esp_err_t key_sink_log_start(key_sink_log_t *l)
{
#if CONFIG_TD_LAZY_KEY_RECORD
    key_sink_log_init(l, NULL, 0, NULL, NULL);
    l->record_on_start = true;
    ESP_LOGI(TAG, "started, record holds %u reports once started", (unsigned)KEY_LOG_RECORD_CAP);
    return ESP_OK;
#else
    key_log_entry_t *entries = heap_caps_malloc(KEY_LOG_RECORD_CAP * sizeof *entries, MALLOC_CAP_SPIRAM);
    key_sink_log_init(l, entries, KEY_LOG_RECORD_CAP, NULL, NULL);
    if (entries == NULL) {
        ESP_LOGE(TAG, "no PSRAM for the record of %u reports", (unsigned)KEY_LOG_RECORD_CAP);
        return ESP_ERR_NO_MEM;
    }
    ESP_LOGI(TAG, "started, record holds %u reports", (unsigned)KEY_LOG_RECORD_CAP);
    return ESP_OK;
#endif
}

static void take(void *ctx, const key_report_t *report)
{
    key_sink_log_t *l = ctx;
    key_log_entry_t e = { .t = l->now_ms(), .report = *report };

    taskENTER_CRITICAL(&l->lock);
    e.seq = ++l->seq;
    if (l->recording) {
        if (l->count < l->cap) {
            l->entries[l->count++] = e;
        } else {
            l->dropped++;
        }
    }
    const bool log_on = l->log_on;
    taskEXIT_CRITICAL(&l->lock);

    if (log_on) {
        char line[KEY_LOG_LINE_MAX];
        key_sink_log_format(&e, line, sizeof line);
        l->print(line);
    }
}

key_sink_t key_sink_log_sink(key_sink_log_t *l)
{
    return (key_sink_t){ .report = take, .ctx = l };
}

typedef struct {
    char *buf;
    size_t cap;
    size_t len;
} fmt_t;

/* Appends like snprintf and stops at the end of the buffer, so a short buffer gets a cut line, never a write
 * past its end. len keeps counting the length the whole line would have. */
static void put(fmt_t *f, const char *fmt, unsigned long a, unsigned long b)
{
    const size_t room = f->len < f->cap ? f->cap - f->len : 0;
    const int n = snprintf(room ? f->buf + f->len : NULL, room, fmt, a, b);
    if (n > 0) {
        f->len += (size_t)n;
    }
}

size_t key_sink_log_format(const key_log_entry_t *e, char *buf, size_t cap)
{
    fmt_t f = { .buf = buf, .cap = cap };
    put(&f, "{\"seq\":%lu,\"t\":%lu,", e->seq, e->t);
    if (e->report.type == KEY_REPORT_CON) {
        put(&f, "\"type\":\"con\",\"usage\":%lu}", e->report.con.usage, 0);
        return f.len;
    }
    put(&f, "\"type\":\"kbd\",\"mods\":%lu,\"keys\":[", e->report.kbd.mods, 0);
    const char *sep = "";
    for (unsigned i = 0; i < KEY_REPORT_KEYS; i++) {
        if (e->report.kbd.keys[i] != 0) {
            put(&f, sep[0] ? ",%lu" : "%lu", e->report.kbd.keys[i], 0);
            sep = ",";
        }
    }
    put(&f, "]}", 0, 0);
    return f.len;
}

void key_sink_log_set_log(key_sink_log_t *l, bool on)
{
    taskENTER_CRITICAL(&l->lock);
    l->log_on = on;
    taskEXIT_CRITICAL(&l->lock);
}

bool key_sink_log_log_on(const key_sink_log_t *l)
{
    return l->log_on;
}

uint32_t key_sink_log_seq(const key_sink_log_t *l)
{
    return l->seq;
}

void key_sink_log_record_start(key_sink_log_t *l)
{
#if CONFIG_TD_LAZY_KEY_RECORD
    /* Only the control calls change entries, and they run on one task, so this check needs no lock. A failed
     * allocation leaves a record of 0, which counts every report in dropped, and the next start tries again. */
    if (l->record_on_start && l->entries == NULL) {
        key_log_entry_t *entries = heap_caps_malloc(KEY_LOG_RECORD_CAP * sizeof *entries, MALLOC_CAP_SPIRAM);
        if (entries == NULL) {
            ESP_LOGE(TAG, "no PSRAM for the record of %u reports", (unsigned)KEY_LOG_RECORD_CAP);
        }
        taskENTER_CRITICAL(&l->lock);
        l->entries = entries;
        l->cap = entries ? KEY_LOG_RECORD_CAP : 0;
        taskEXIT_CRITICAL(&l->lock);
    }
#endif
    taskENTER_CRITICAL(&l->lock);
    l->count = 0;
    l->dropped = 0;
    l->recording = true;
    taskEXIT_CRITICAL(&l->lock);
}

void key_sink_log_record_stop(key_sink_log_t *l)
{
    taskENTER_CRITICAL(&l->lock);
    l->recording = false;
    taskEXIT_CRITICAL(&l->lock);
}

bool key_sink_log_recording(const key_sink_log_t *l)
{
    return l->recording;
}

size_t key_sink_log_count(const key_sink_log_t *l)
{
    return l->count;
}

uint32_t key_sink_log_dropped(const key_sink_log_t *l)
{
    return l->dropped;
}

const key_log_entry_t *key_sink_log_entry(const key_sink_log_t *l, size_t i)
{
    return &l->entries[i];
}
