#include "display.h"
#include "display_perf.h"
#include "display_priv.h"
#include <stdarg.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "driver/temperature_sensor.h"
#include "esp_heap_caps.h"
#include "esp_lv_adapter_display.h"
#include "esp_rom_sys.h"
#include "esp_system.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/freertos_debug.h"
#include "freertos/task.h"
#include "sdkconfig.h"
#include "td_console.h"

#if !configTASKLIST_INCLUDE_COREID
#error "perf mem and perf cpu need CONFIG_FREERTOS_VTASKLIST_INCLUDE_COREID for each task's core"
#endif

#define CPU_MAX_SECONDS 60
#define CPU_TASK_STACK 4096
#define CPU_TASK_PRIORITY 2
#define INFO_REPLY_MAX 768
/* perf mem and perf cpu list every task on one line, about 70 bytes a task. */
#define LIST_REPLY_MAX 6144
#define LIST_SPARE_TASKS 8
/* A tap whose press or release has not redrawn within this long gets null for it. */
#define TAP_LIMIT_US 500000
#define TAP_GIVE_UP_MS 1000
#define TAP_POLL_MS 1
#define STACK_ALIGN 16

#ifdef CONFIG_SPIRAM_ECC_ENABLE
#define PSRAM_ECC true
#else
#define PSRAM_ECC false
#endif
#ifdef CONFIG_LV_ATTRIBUTE_FAST_MEM_USE_IRAM
#define LV_FAST_MEM_IRAM true
#else
#define LV_FAST_MEM_IRAM false
#endif
#ifdef CONFIG_SPIRAM_XIP_FROM_PSRAM
#define SPIRAM_XIP true
#else
#define SPIRAM_XIP false
#endif
#ifdef CONFIG_TD_LAZY_SNAPSHOT_BUF
#define LAZY_SNAPSHOT_BUF true
#else
#define LAZY_SNAPSHOT_BUF false
#endif
#ifdef CONFIG_TD_LAZY_KEY_RECORD
#define LAZY_KEY_RECORD true
#else
#define LAZY_KEY_RECORD false
#endif
#ifdef CONFIG_TD_PAY_COPY_DEBT
#define PAY_COPY_DEBT true
#else
#define PAY_COPY_DEBT false
#endif
/* The sensor range with the smallest error, under 1 degree, that covers a working board. */
#define TEMP_RANGE_MIN_C (-10)
#define TEMP_RANGE_MAX_C 80

#if defined(CONFIG_LV_DRAW_SW_ASM_NONE)
#define LV_DRAW_SW_ASM_NAME "none"
#elif defined(CONFIG_LV_DRAW_SW_ASM_NEON)
#define LV_DRAW_SW_ASM_NAME "neon"
#elif defined(CONFIG_LV_DRAW_SW_ASM_HELIUM)
#define LV_DRAW_SW_ASM_NAME "helium"
#elif defined(CONFIG_LV_DRAW_SW_ASM_RISCV_V)
#define LV_DRAW_SW_ASM_NAME "riscv_v"
#elif defined(CONFIG_LV_DRAW_SW_ASM_SVE2)
#define LV_DRAW_SW_ASM_NAME "sve2"
#elif defined(CONFIG_LV_DRAW_SW_ASM_CUSTOM)
#define LV_DRAW_SW_ASM_NAME "custom:" CONFIG_LV_DRAW_SW_ASM_CUSTOM_INCLUDE
#else
#define LV_DRAW_SW_ASM_NAME "unknown"
#endif

static atomic_bool s_cpu_busy;

/* The console runs one command at a time, so only its task touches the window. */
static struct {
    bool open;
    display_perf_mark_t mark;
} s_window;

void display_perf_send(td_reply_t *r)
{
    td_reply_end(r);
    td_console_write_line(r->buf);
}

void display_perf_send_error(const char *message)
{
    char buf[TD_REPLY_MAX];
    td_reply_error(buf, sizeof buf, "perf", message);
    td_console_write_line(buf);
}

void display_perf_reply_printf(td_reply_t *r, const char *fmt, ...)
{
    if (r->overflow) {
        return;
    }
    const size_t room = r->cap - r->len;
    va_list ap;
    va_start(ap, fmt);
    const int n = vsnprintf(r->buf + r->len, room, fmt, ap);
    va_end(ap);
    if (n < 0 || (size_t)n >= room) {
        r->overflow = true;
        r->buf[r->len] = '\0';
        return;
    }
    r->len += (size_t)n;
}

void display_perf_reply_null(td_reply_t *r, const char *key)
{
    display_perf_reply_printf(r, ",\"%s\":null", key);
}

static void reply_hundredths(td_reply_t *r, const char *key, uint32_t hundredths)
{
    display_perf_reply_printf(r, ",\"%s\":%lu.%02lu", key, (unsigned long)(hundredths / 100),
                              (unsigned long)(hundredths % 100));
}

void display_perf_reply_tenths(td_reply_t *r, const char *key, int32_t tenths)
{
    if (tenths < 0) {
        display_perf_reply_null(r, key);
        return;
    }
    display_perf_reply_printf(r, ",\"%s\":%ld.%ld", key, (long)(tenths / 10), (long)(tenths % 10));
}

static void reply_uint_or_null(td_reply_t *r, const char *key, bool valid, uint32_t us)
{
    if (valid) {
        td_reply_int(r, key, us);
    } else {
        display_perf_reply_null(r, key);
    }
}

/* FreeRTOS names are plain text in practice. Anything that would need escaping becomes '?'. */
void display_perf_reply_task_name(td_reply_t *r, const char *name)
{
    char clean[configMAX_TASK_NAME_LEN + 1];
    size_t i = 0;
    for (; name[i] != '\0' && i < configMAX_TASK_NAME_LEN; i++) {
        const unsigned char c = (unsigned char)name[i];
        clean[i] = (c < 0x20 || c > 0x7e || c == '"' || c == '\\') ? '?' : (char)c;
    }
    clean[i] = '\0';
    display_perf_reply_printf(r, "{\"name\":\"%s\"", clean);
}

/* The core a task is pinned to, -1 for none. uxTaskGetSystemState fills it in with the rest, so no TCB is read
 * after the snapshot. */
static int task_core(const TaskStatus_t *t)
{
    return t->xCoreID == tskNO_AFFINITY ? -1 : (int)t->xCoreID;
}

void display_perf_reply_window(td_reply_t *r, const display_perf_window_t *w)
{
    td_reply_int(r, "shown", w->shown);
    reply_hundredths(r, "shown_fps", w->elapsed_ms ? (uint32_t)((uint64_t)w->shown * 100000 / w->elapsed_ms) : 0);
    td_reply_int(r, "refreshes", w->refreshes);
    reply_uint_or_null(r, "render_us_avg", w->render_n > 0, w->render_us_avg);
    reply_uint_or_null(r, "render_us_max", w->render_n > 0, w->render_us_max);
    reply_uint_or_null(r, "render_only_us_avg", w->render_n > 0, w->render_only_us_avg);
    reply_uint_or_null(r, "render_only_us_max", w->render_n > 0, w->render_only_us_max);
    reply_uint_or_null(r, "copy_us_avg", w->flush_n > 0, w->copy_us_avg);
    reply_uint_or_null(r, "copy_us_max", w->flush_n > 0, w->copy_us_max);
    reply_uint_or_null(r, "flush_us_avg", w->flush_n > 0, w->flush_us_avg);
    reply_uint_or_null(r, "flush_us_max", w->flush_n > 0, w->flush_us_max);
    td_reply_int(r, "px", (long long)w->px);
    display_perf_reply_tenths(r, "cpu0_pct", w->cpu_x10[0]);
    display_perf_reply_tenths(r, "cpu1_pct", DISPLAY_PERF_CORES > 1 ? w->cpu_x10[DISPLAY_PERF_CORES - 1] : -1);
}

static const char *tear_mode_name(int mode)
{
    switch (mode) {
    case ESP_LV_ADAPTER_TEAR_AVOID_MODE_NONE:
        return "none";
    case ESP_LV_ADAPTER_TEAR_AVOID_MODE_DOUBLE_FULL:
        return "double_full";
    case ESP_LV_ADAPTER_TEAR_AVOID_MODE_TRIPLE_FULL:
        return "triple_full";
    case ESP_LV_ADAPTER_TEAR_AVOID_MODE_DOUBLE_DIRECT:
        return "double_direct";
    case ESP_LV_ADAPTER_TEAR_AVOID_MODE_TRIPLE_PARTIAL:
        return "triple_partial";
    case ESP_LV_ADAPTER_TEAR_AVOID_MODE_TE_SYNC:
        return "te_sync";
    case ESP_LV_ADAPTER_TEAR_AVOID_MODE_DOUBLE_PARTIAL:
        return "double_partial";
    default:
        return "unknown";
    }
}

static const char *reset_reason_name(esp_reset_reason_t reason)
{
    switch (reason) {
    case ESP_RST_POWERON:
        return "poweron";
    case ESP_RST_EXT:
        return "ext";
    case ESP_RST_SW:
        return "sw";
    case ESP_RST_PANIC:
        return "panic";
    case ESP_RST_INT_WDT:
        return "int_wdt";
    case ESP_RST_TASK_WDT:
        return "task_wdt";
    case ESP_RST_WDT:
        return "wdt";
    case ESP_RST_DEEPSLEEP:
        return "deepsleep";
    case ESP_RST_BROWNOUT:
        return "brownout";
    case ESP_RST_SDIO:
        return "sdio";
    case ESP_RST_USB:
        return "usb";
    case ESP_RST_JTAG:
        return "jtag";
    case ESP_RST_EFUSE:
        return "efuse";
    case ESP_RST_PWR_GLITCH:
        return "pwr_glitch";
    case ESP_RST_CPU_LOCKUP:
        return "cpu_lockup";
    default:
        return "unknown";
    }
}

static void cmd_info(void)
{
    const display_perf_config_t *c = display_perf_config();
    char buf[INFO_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "perf", true);
    td_reply_str(&r, "op", "info");
    td_reply_str(&r, "variant", CONFIG_TD_PERF_VARIANT);
    td_reply_int(&r, "psram_mhz", CONFIG_SPIRAM_SPEED);
    td_reply_bool(&r, "psram_ecc", PSRAM_ECC);
    td_reply_bool(&r, "spiram_xip", SPIRAM_XIP);
    td_reply_int(&r, "flash_mhz", strtol(CONFIG_ESPTOOLPY_FLASHFREQ, NULL, 10));
    td_reply_int(&r, "cpu_mhz", esp_rom_get_cpu_ticks_per_us());
    td_reply_int(&r, "pclk_hz", c->pclk_hz);
    td_reply_int(&r, "bounce_lines", c->bounce_lines);
    td_reply_int(&r, "num_fbs", c->num_fbs);
    td_reply_str(&r, "tear_mode", tear_mode_name(c->tear_mode));
    td_reply_int(&r, "rotation", c->rotation);
    td_reply_int(&r, "lv_draw_units", CONFIG_LV_DRAW_SW_DRAW_UNIT_CNT);
    td_reply_str(&r, "lv_draw_sw_asm", LV_DRAW_SW_ASM_NAME);
    td_reply_int(&r, "lv_refr_period_ms", CONFIG_LV_DEF_REFR_PERIOD);
    td_reply_bool(&r, "lv_fast_mem_iram", LV_FAST_MEM_IRAM);
    td_reply_int(&r, "dcache_kb", CONFIG_ESP32S3_DATA_CACHE_SIZE / 1024);
    td_reply_int(&r, "dcache_line", CONFIG_ESP32S3_DATA_CACHE_LINE_SIZE);
    td_reply_int(&r, "icache_kb", CONFIG_ESP32S3_INSTRUCTION_CACHE_SIZE / 1024);
    td_reply_bool(&r, "lv_tick_cb", lv_tick_get_cb() != NULL);
    td_reply_bool(&r, "lazy_snapshot_buf", LAZY_SNAPSHOT_BUF);
    td_reply_bool(&r, "lazy_key_record", LAZY_KEY_RECORD);
    td_reply_bool(&r, "pay_copy_debt", PAY_COPY_DEBT);
#if CONFIG_TD_PAY_COPY_DEBT
    td_reply_int(&r, "copy_debt_steps", display_copy_debt_paid());
#endif
    td_reply_str(&r, "reset_reason", reset_reason_name(esp_reset_reason()));
    td_reply_int(&r, "boot_count", display_perf_boot_count());
    display_perf_send(&r);
}

/* start begins a new window, also over one left open. Neither forces a redraw. */
static void cmd_window(int argc, char **argv)
{
    const bool start = argc == 3 && strcmp(argv[2], "start") == 0;
    if (!start && !(argc == 3 && strcmp(argv[2], "stop") == 0)) {
        display_perf_send_error("usage: perf window start|stop");
        return;
    }

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    if (start) {
        display_perf_window_begin(DISPLAY_PERF_SLOT_WINDOW, &s_window.mark);
        s_window.open = true;
        td_reply_begin(&r, buf, sizeof buf, "perf", true);
        td_reply_str(&r, "op", "window");
        td_reply_str(&r, "phase", "start");
        display_perf_send(&r);
        return;
    }
    if (!s_window.open) {
        display_perf_send_error("no window");
        return;
    }
    display_perf_window_t w;
    display_perf_window_end(DISPLAY_PERF_SLOT_WINDOW, &s_window.mark, &w);
    s_window.open = false;

    td_reply_begin(&r, buf, sizeof buf, "perf", true);
    td_reply_str(&r, "op", "window");
    td_reply_str(&r, "phase", "stop");
    td_reply_int(&r, "elapsed_ms", w.elapsed_ms);
    td_reply_int(&r, "drawn", w.drawn);
    display_perf_reply_window(&r, &w);
    display_perf_send(&r);
}

bool display_perf_parse_uint(const char *word, uint32_t min, uint32_t max, uint32_t *out)
{
    if (word[0] < '0' || word[0] > '9') {
        return false;
    }
    char *end = NULL;
    const unsigned long value = strtoul(word, &end, 10);
    if (*end != '\0' || value < min || value > max) {
        return false;
    }
    *out = (uint32_t)value;
    return true;
}

static bool parse_coord(const char *word, uint32_t max, uint32_t *out)
{
    return display_perf_parse_uint(word, 0, max, out);
}

static const char *tap_error(esp_err_t err)
{
    switch (err) {
    case ESP_ERR_INVALID_STATE:
        return "finger down";
    case ESP_ERR_NO_MEM:
        return "queue full";
    case ESP_ERR_TIMEOUT:
        return "not consumed in time";
    case ESP_ERR_NOT_FINISHED:
        return "busy";
    default:
        return "inject failed";
    }
}

/* Done when the probe has both times, or when its redraw is overdue. */
static bool probe_settled(const display_perf_probe_t *p, uint32_t now)
{
    const uint32_t both = DISPLAY_PERF_PROBE_FLUSHED | DISPLAY_PERF_PROBE_SHOWN;
    return (p->flags & DISPLAY_PERF_PROBE_SAMPLED) &&
           ((p->flags & both) == both || now - p->sample_us > TAP_LIMIT_US);
}

/* <name>_to_flush_us and <name>_to_shown_us, then the refresh that flushed: its render and copy times, strips and
 * pixels. */
static void reply_probe(td_reply_t *r, const char *name, const display_perf_probe_t *p)
{
    char key[32];
    const bool sampled = (p->flags & DISPLAY_PERF_PROBE_SAMPLED) != 0;
    const uint32_t to_flush = p->flush_us - p->sample_us;
    const uint32_t to_shown = p->shown_us - p->sample_us;
    const bool flushed = sampled && (p->flags & DISPLAY_PERF_PROBE_FLUSHED) && to_flush <= TAP_LIMIT_US;
    snprintf(key, sizeof key, "%s_to_flush_us", name);
    reply_uint_or_null(r, key, flushed, to_flush);
    snprintf(key, sizeof key, "%s_to_shown_us", name);
    reply_uint_or_null(r, key, sampled && (p->flags & DISPLAY_PERF_PROBE_SHOWN) && to_shown <= TAP_LIMIT_US, to_shown);
    snprintf(key, sizeof key, "%s_render_us", name);
    reply_uint_or_null(r, key, flushed, p->render_us);
    snprintf(key, sizeof key, "%s_copy_us", name);
    reply_uint_or_null(r, key, flushed, p->copy_us);
    snprintf(key, sizeof key, "%s_strips", name);
    reply_uint_or_null(r, key, flushed, p->strips);
    snprintf(key, sizeof key, "%s_px", name);
    reply_uint_or_null(r, key, flushed, p->px);
}

/* Blocks the console like `touch tap`, for the tap and then up to TAP_LIMIT_US for its redraws. */
static void cmd_tap(int argc, char **argv)
{
    uint32_t x = 0;
    uint32_t y = 0;
    if (argc != 4 || !parse_coord(argv[2], display_width() - 1U, &x) ||
        !parse_coord(argv[3], display_height() - 1U, &y)) {
        display_perf_send_error("usage: perf tap <x> <y>");
        return;
    }
    const display_perf_tap_fn tap = display_perf_tap();
    if (tap == NULL) {
        display_perf_send_error("touch not ready");
        return;
    }

    display_perf_probe_arm();
    uint32_t deliver_us = 0;
    const esp_err_t err = tap((uint16_t)x, (uint16_t)y, &deliver_us);
    if (err != ESP_OK) {
        display_perf_probe_disarm();
        display_perf_send_error(tap_error(err));
        return;
    }
    display_perf_probe_t probes[DISPLAY_PERF_PROBES];
    const int64_t start = esp_timer_get_time();
    while (true) {
        display_perf_probe_read(probes);
        const uint32_t now = (uint32_t)esp_timer_get_time();
        bool settled = true;
        for (size_t i = 0; i < DISPLAY_PERF_PROBES; i++) {
            settled = settled && probe_settled(&probes[i], now);
        }
        if (settled || esp_timer_get_time() - start > TAP_GIVE_UP_MS * 1000LL) {
            break;
        }
        vTaskDelay(pdMS_TO_TICKS(TAP_POLL_MS));
    }
    display_perf_probe_disarm();

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "perf", true);
    td_reply_str(&r, "op", "tap");
    td_reply_int(&r, "x", x);
    td_reply_int(&r, "y", y);
    td_reply_int(&r, "deliver_us", deliver_us);
    /* The frame-complete wrapper calls the adapter's own callback after its count, so shown is taken without
     * changing what the adapter does. */
    td_reply_bool(&r, "shown_chained", true);
    reply_probe(&r, "press", &probes[DISPLAY_PERF_PROBE_PRESS]);
    reply_probe(&r, "release", &probes[DISPLAY_PERF_PROBE_RELEASE]);
    display_perf_send(&r);
}

void *display_perf_alloc_scratch(size_t size)
{
    return heap_caps_malloc_prefer(size, 2, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT, MALLOC_CAP_8BIT);
}

static void reply_heap(td_reply_t *r, const char *name, uint32_t caps)
{
    char key[32];
    snprintf(key, sizeof key, "free_%s", name);
    td_reply_int(r, key, heap_caps_get_free_size(caps));
    snprintf(key, sizeof key, "min_free_%s", name);
    td_reply_int(r, key, heap_caps_get_minimum_free_size(caps));
    snprintf(key, sizeof key, "largest_%s", name);
    td_reply_int(r, key, heap_caps_get_largest_free_block(caps));
}

static void sort_by_number(TaskStatus_t *tasks, size_t n)
{
    for (size_t i = 1; i < n; i++) {
        const TaskStatus_t t = tasks[i];
        size_t j = i;
        for (; j > 0 && tasks[j - 1].xTaskNumber > t.xTaskNumber; j--) {
            tasks[j] = tasks[j - 1];
        }
        tasks[j] = t;
    }
}

/* A task's stack runs from its base to the top FreeRTOS aligned down to STACK_ALIGN, so rounding the span up gives
 * the size it was created with when that is a multiple of STACK_ALIGN, and within STACK_ALIGN bytes otherwise. */
static uint32_t stack_size(const TaskStatus_t *t)
{
    TaskSnapshot_t snap;
    if (vTaskGetSnapshot(t->xHandle, &snap) != pdTRUE || snap.pxEndOfStack < t->pxStackBase) {
        return 0;
    }
    const uintptr_t span = (uintptr_t)snap.pxEndOfStack - (uintptr_t)t->pxStackBase + 1;
    return (uint32_t)((span + STACK_ALIGN - 1) / STACK_ALIGN * STACK_ALIGN);
}

static void cmd_mem(void)
{
    char *buf = display_perf_alloc_scratch(LIST_REPLY_MAX);
    const UBaseType_t cap = uxTaskGetNumberOfTasks() + LIST_SPARE_TASKS;
    TaskStatus_t *tasks = display_perf_alloc_scratch(cap * sizeof *tasks);
    uint32_t *stacks = display_perf_alloc_scratch(cap * sizeof *stacks);
    if (buf == NULL || tasks == NULL || stacks == NULL) {
        free(buf);
        free(tasks);
        free(stacks);
        display_perf_send_error("no memory");
        return;
    }

    /* The heap figures include this command's own scratch, about 7 KB of PSRAM. vTaskGetSnapshot reads each TCB, so
     * the scheduler stays off this core until the stack sizes are in. */
    td_reply_t r;
    td_reply_begin(&r, buf, LIST_REPLY_MAX, "perf", true);
    td_reply_str(&r, "op", "mem");
    reply_heap(&r, "internal", MALLOC_CAP_INTERNAL);
    reply_heap(&r, "psram", MALLOC_CAP_SPIRAM);
    reply_heap(&r, "dma", MALLOC_CAP_DMA);

    vTaskSuspendAll();
    const UBaseType_t n = uxTaskGetSystemState(tasks, cap, NULL);
    sort_by_number(tasks, n);
    for (UBaseType_t i = 0; i < n; i++) {
        stacks[i] = stack_size(&tasks[i]);
    }
    (void)xTaskResumeAll();

    td_reply_int(&r, "tasks_n", n);
    display_perf_reply_printf(&r, ",\"tasks\":[");
    for (UBaseType_t i = 0; i < n; i++) {
        display_perf_reply_printf(&r, "%s", i == 0 ? "" : ",");
        display_perf_reply_task_name(&r, tasks[i].pcTaskName);
        display_perf_reply_printf(&r, ",\"core\":%d,\"prio\":%u", task_core(&tasks[i]),
                                  (unsigned)tasks[i].uxCurrentPriority);
        if (stacks[i] != 0) {
            td_reply_int(&r, "stack", stacks[i]);
        } else {
            display_perf_reply_null(&r, "stack");
        }
        td_reply_int(&r, "hwm", tasks[i].usStackHighWaterMark);
        display_perf_reply_printf(&r, "}");
    }
    display_perf_reply_printf(&r, "]");
    display_perf_send(&r);
    free(stacks);
    free(tasks);
    free(buf);
}

static void cpu_task(void *arg)
{
    const uint32_t seconds = (uint32_t)(uintptr_t)arg;
    const UBaseType_t cap = uxTaskGetNumberOfTasks() + LIST_SPARE_TASKS;
    TaskStatus_t *before = display_perf_alloc_scratch(cap * sizeof *before);
    TaskStatus_t *after = display_perf_alloc_scratch(cap * sizeof *after);
    char *buf = display_perf_alloc_scratch(LIST_REPLY_MAX);
    if (before == NULL || after == NULL || buf == NULL) {
        display_perf_send_error("no memory");
    } else {
        display_perf_core_t cores0[DISPLAY_PERF_CORES];
        display_perf_core_t cores1[DISPLAY_PERF_CORES];
        display_perf_sample_cores(cores0);
        const UBaseType_t n0 = uxTaskGetSystemState(before, cap, NULL);
        const int64_t t0 = esp_timer_get_time();
        vTaskDelay(pdMS_TO_TICKS(seconds * 1000));
        display_perf_sample_cores(cores1);
        const UBaseType_t n1 = uxTaskGetSystemState(after, cap, NULL);
        const int64_t t1 = esp_timer_get_time();
        const uint64_t span = (uint64_t)(t1 - t0);

        /* Each entry's run time becomes its run time in the window, then the list goes busiest first. A task that
         * started during the window counts from 0, and one that ended during it is gone. */
        for (UBaseType_t i = 0; i < n1; i++) {
            configRUN_TIME_COUNTER_TYPE start = 0;
            for (UBaseType_t j = 0; j < n0; j++) {
                if (before[j].xHandle == after[i].xHandle && before[j].xTaskNumber == after[i].xTaskNumber) {
                    start = before[j].ulRunTimeCounter;
                    break;
                }
            }
            after[i].ulRunTimeCounter = (configRUN_TIME_COUNTER_TYPE)(after[i].ulRunTimeCounter - start);
        }
        for (UBaseType_t i = 1; i < n1; i++) {
            const TaskStatus_t t = after[i];
            UBaseType_t j = i;
            for (; j > 0 && after[j - 1].ulRunTimeCounter < t.ulRunTimeCounter; j--) {
                after[j] = after[j - 1];
            }
            after[j] = t;
        }

        td_reply_t r;
        td_reply_begin(&r, buf, LIST_REPLY_MAX, "perf", true);
        td_reply_str(&r, "op", "cpu");
        td_reply_int(&r, "seconds", seconds);
        td_reply_int(&r, "elapsed_ms", (long long)(span / 1000));
        display_perf_reply_tenths(&r, "cpu0_pct", display_perf_busy_x10(&cores0[0], &cores1[0]));
        const int last = DISPLAY_PERF_CORES - 1;
        display_perf_reply_tenths(&r, "cpu1_pct",
                                  last > 0 ? display_perf_busy_x10(&cores0[last], &cores1[last]) : -1);
        display_perf_reply_printf(&r, ",\"tasks\":[");
        for (UBaseType_t i = 0; i < n1; i++) {
            const uint64_t run = after[i].ulRunTimeCounter;
            display_perf_reply_printf(&r, "%s", i == 0 ? "" : ",");
            display_perf_reply_task_name(&r, after[i].pcTaskName);
            display_perf_reply_printf(&r, ",\"core\":%d,\"prio\":%u", task_core(&after[i]),
                                      (unsigned)after[i].uxCurrentPriority);
            display_perf_reply_tenths(&r, "pct", span ? (int32_t)(run * 1000 / span) : -1);
            display_perf_reply_printf(&r, "}");
        }
        display_perf_reply_printf(&r, "]");
        display_perf_send(&r);
    }
    free(buf);
    free(after);
    free(before);
    atomic_store(&s_cpu_busy, false);
    vTaskDelete(NULL);
}

/* The reply comes when the window ends, so the console stays free meanwhile, as with `perf fps`. */
static void cmd_cpu(int argc, char **argv)
{
    char *end = NULL;
    const unsigned long seconds = argc == 3 ? strtoul(argv[2], &end, 10) : 0;
    if (argc != 3 || *argv[2] == '\0' || *end != '\0' || seconds < 1 || seconds > CPU_MAX_SECONDS) {
        display_perf_send_error("usage: perf cpu <seconds 1-60>");
        return;
    }
    if (atomic_exchange(&s_cpu_busy, true)) {
        display_perf_send_error("perf cpu already running");
        return;
    }
    if (xTaskCreate(cpu_task, "td_perf_cpu", CPU_TASK_STACK, (void *)(uintptr_t)seconds, CPU_TASK_PRIORITY, NULL) !=
        pdPASS) {
        atomic_store(&s_cpu_busy, false);
        display_perf_send_error("cannot start the perf task");
    }
}

/* Keep the installed sensor until reset, including after an enable failure. Only the console task uses it. */
static temperature_sensor_handle_t s_temp_sensor;
static bool s_temp_enabled;

static esp_err_t temp_sensor_start(void)
{
    if (s_temp_sensor == NULL) {
        const temperature_sensor_config_t config = TEMPERATURE_SENSOR_CONFIG_DEFAULT(TEMP_RANGE_MIN_C, TEMP_RANGE_MAX_C);
        esp_err_t err = temperature_sensor_install(&config, &s_temp_sensor);
        if (err != ESP_OK) {
            return err;
        }
    }
    if (!s_temp_enabled) {
        esp_err_t err = temperature_sensor_enable(s_temp_sensor);
        if (err != ESP_OK) {
            return err;
        }
        s_temp_enabled = true;
    }
    return ESP_OK;
}

static void cmd_temp(void)
{
    float celsius = 0;
    esp_err_t err = temp_sensor_start();
    if (err == ESP_OK) {
        err = temperature_sensor_get_celsius(s_temp_sensor, &celsius);
    }
    if (err != ESP_OK) {
        display_perf_send_error(esp_err_to_name(err));
        return;
    }

    const int32_t tenths = (int32_t)(celsius * 10.0f + (celsius < 0 ? -0.5f : 0.5f));
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "perf", true);
    td_reply_str(&r, "op", "temp");
    display_perf_reply_printf(&r, ",\"temp_c\":%s%ld.%ld", tenths < 0 ? "-" : "", (long)(labs(tenths) / 10),
                              (long)(labs(tenths) % 10));
    display_perf_send(&r);
}

int display_perf_command(int argc, char **argv)
{
    if (argc == 2 && strcmp(argv[1], "info") == 0) {
        cmd_info();
    } else if (argc >= 2 && strcmp(argv[1], "window") == 0) {
        cmd_window(argc, argv);
    } else if (argc >= 2 && strcmp(argv[1], "tap") == 0) {
        cmd_tap(argc, argv);
    } else if (argc == 2 && strcmp(argv[1], "mem") == 0) {
        cmd_mem();
    } else if (argc == 3 && strcmp(argv[1], "mem") == 0 && strcmp(argv[2], "boot") == 0) {
        display_perf_cmd_mem_boot();
    } else if (argc == 3 && strcmp(argv[1], "mem") == 0 && strcmp(argv[2], "last") == 0) {
        display_perf_cmd_mem_last();
    } else if (argc == 3 && strcmp(argv[1], "mem") == 0 && strcmp(argv[2], "heaps") == 0) {
        display_perf_cmd_mem_heaps();
    } else if (argc >= 2 && strcmp(argv[1], "cpu") == 0) {
        cmd_cpu(argc, argv);
    } else if (argc >= 2 && strcmp(argv[1], "fb") == 0) {
        display_perf_cmd_fb(argc, argv);
    } else if (argc >= 2 && strcmp(argv[1], "invalidate") == 0) {
        display_perf_cmd_invalidate(argc, argv);
    } else if (argc >= 2 && strcmp(argv[1], "stall") == 0) {
        display_perf_cmd_stall(argc, argv);
    } else if (argc >= 2 && strcmp(argv[1], "memtest") == 0) {
        display_perf_cmd_memtest(argc, argv);
    } else if (argc == 2 && strcmp(argv[1], "membw") == 0) {
        display_perf_cmd_membw();
    } else if (argc == 2 && strcmp(argv[1], "temp") == 0) {
        cmd_temp();
    } else {
        display_perf_send_error(DISPLAY_PERF_USAGE);
    }
    return 0;
}
