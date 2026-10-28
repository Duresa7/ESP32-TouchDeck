#include "display_perf.h"
#include "display_priv.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "esp_attr.h"
#include "esp_cache.h"
#include "esp_heap_caps.h"
#include "esp_rom_crc.h"
#include "esp_system.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/idf_additions.h"
#include "freertos/task.h"

/*
 * Memory figures for the perf tools.
 *
 * Boot checkpoints: app_main and display_start call display_perf_boot_mark after each stage, and `perf mem boot`
 * lists the free and minimum free internal RAM after each, so a drop in the minimum points at the stage that
 * caused it.
 *
 * Last boot: a shutdown handler, which esp_restart runs, saves the minimum free size of each heap and every task's
 * stack high-water mark to RTC memory that no reset clears, under a magic number and a CRC, and `perf mem last`
 * reports them after the restart. A panic, a watchdog or a power cut runs no shutdown handler, so the record then
 * stays the one an earlier restart saved, and `perf mem last` answers saved false with that boot in stale_boot.
 *
 * The boot count lives in that memory too, and starts again from 1 when power-on leaves it holding garbage.
 */

#define BOOT_STAGES_MAX 24
#define LAST_TASKS_MAX 24
#define BOOT_MAGIC 0x54444243u
#define LAST_MAGIC 0x54444c4du
#define MEM_REPLY_MAX 3072

#define MEMTEST_MAX_KB 4096
#define MEMTEST_ALIGN 64
#define MEMTEST_PATTERNS 6

#define MEMBW_PSRAM_BYTES (512 * 1024)
#define MEMBW_SRAM_BYTES 4096
#define MEMBW_PASSES 4
#define MEMBW_ALIGN 64
#define MEMBW_CORE 1
#define MEMBW_STACK 3072
#define MEMBW_PRIORITY 5

typedef struct {
    const char *stage;
    uint32_t ms;
    uint32_t free_internal;
    uint32_t min_free_internal;
    uint32_t largest_internal;
    uint32_t free_psram;
} boot_stage_t;

typedef struct {
    uint32_t magic;
    uint32_t count;
    uint32_t check;
} boot_count_t;

typedef struct {
    char name[configMAX_TASK_NAME_LEN];
    uint32_t hwm;
} last_task_t;

/* crc covers every field after it. */
typedef struct {
    uint32_t magic;
    uint32_t crc;
    uint32_t boot;
    uint32_t uptime_ms;
    uint32_t free_internal;
    uint32_t min_free_internal;
    uint32_t free_psram;
    uint32_t min_free_psram;
    uint32_t free_dma;
    uint32_t min_free_dma;
    uint32_t tasks_n;
    last_task_t tasks[LAST_TASKS_MAX];
} last_mem_t;

/* RTC slow memory, outside the heaps, so the records cost the heaps nothing. */
static RTC_NOINIT_ATTR boot_count_t s_boot_count;
static RTC_NOINIT_ATTR last_mem_t s_last;
static RTC_NOINIT_ATTR boot_stage_t s_stages[BOOT_STAGES_MAX];
static uint32_t s_stages_n;
static bool s_boot_started;
/* save_last's task list, allocated at app_main, so the shutdown handler allocates nothing. */
static TaskStatus_t *s_last_status;

static uint32_t last_crc(const last_mem_t *l)
{
    const uint8_t *from = (const uint8_t *)&l->boot;
    return esp_rom_crc32_le(0, from, (uint32_t)((const uint8_t *)(l + 1) - from));
}

/*
 * Runs in esp_restart, on the task that called it, while the other core still runs, so it keeps to a few hundred
 * bytes of stack and takes no lock but the kernel's. The heap figures are counters it reads without a lock.
 * uxTaskGetSystemState walks the task lists and reads each stack's high-water mark under the kernel lock, so no task
 * moves between lists during the walk, which an unlocked walk with only this core's scheduler suspended allowed. It
 * lists no task when more exist than the array holds.
 */
static void save_last(void)
{
    last_mem_t *l = &s_last;
    l->magic = 0;
    l->boot = s_boot_count.count;
    l->uptime_ms = (uint32_t)(esp_timer_get_time() / 1000);
    l->free_internal = heap_caps_get_free_size(MALLOC_CAP_INTERNAL);
    l->min_free_internal = heap_caps_get_minimum_free_size(MALLOC_CAP_INTERNAL);
    l->free_psram = heap_caps_get_free_size(MALLOC_CAP_SPIRAM);
    l->min_free_psram = heap_caps_get_minimum_free_size(MALLOC_CAP_SPIRAM);
    l->free_dma = heap_caps_get_free_size(MALLOC_CAP_DMA);
    l->min_free_dma = heap_caps_get_minimum_free_size(MALLOC_CAP_DMA);

    const UBaseType_t n = s_last_status != NULL ? uxTaskGetSystemState(s_last_status, LAST_TASKS_MAX, NULL) : 0;
    for (UBaseType_t i = 0; i < n; i++) {
        strlcpy(l->tasks[i].name, s_last_status[i].pcTaskName, sizeof l->tasks[i].name);
        l->tasks[i].hwm = s_last_status[i].usStackHighWaterMark;
    }
    l->tasks_n = n;
    l->crc = last_crc(l);
    l->magic = LAST_MAGIC;
}

void display_perf_boot_mark(const char *stage)
{
    if (!s_boot_started) {
        s_boot_started = true;
        s_stages_n = 0;
        s_last_status = display_perf_alloc_scratch(LAST_TASKS_MAX * sizeof *s_last_status);
        /* ESP-IDF adds the 32 KB internal pool of CONFIG_SPIRAM_MALLOC_RESERVE_INTERNAL after its startup, and gives a
         * heap added then a minimum free size of 0, so the internal minimum read 32 KB below anything ever in use.
         * This starts every heap's minimum from its free size at app_main, before any of the deck's allocations. */
        (void)heap_caps_monitor_local_minimum_free_size_start();
        const bool counted = s_boot_count.magic == BOOT_MAGIC && s_boot_count.check == ~s_boot_count.count;
        s_boot_count.count = counted ? s_boot_count.count + 1 : 1;
        s_boot_count.check = ~s_boot_count.count;
        s_boot_count.magic = BOOT_MAGIC;
        (void)esp_register_shutdown_handler(save_last);
    }
    if (s_stages_n >= BOOT_STAGES_MAX) {
        return;
    }
    s_stages[s_stages_n++] = (boot_stage_t){
        .stage = stage,
        .ms = (uint32_t)(esp_timer_get_time() / 1000),
        .free_internal = heap_caps_get_free_size(MALLOC_CAP_INTERNAL),
        .min_free_internal = heap_caps_get_minimum_free_size(MALLOC_CAP_INTERNAL),
        .largest_internal = heap_caps_get_largest_free_block(MALLOC_CAP_INTERNAL),
        .free_psram = heap_caps_get_free_size(MALLOC_CAP_SPIRAM),
    };
}

uint32_t display_perf_boot_count(void)
{
    return s_boot_started ? s_boot_count.count : 0;
}

/* ESP-IDF keeps its list of heaps private, so the per-heap figures go out as its own log text, one "At 0x..." line
 * pair a heap, before the reply. A minimum far below a heap's free size names the heap that a past allocation
 * emptied. */
void display_perf_cmd_mem_heaps(void)
{
    heap_caps_print_heap_info(MALLOC_CAP_INTERNAL);
    fflush(stdout);
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "perf", true);
    td_reply_str(&r, "op", "mem");
    td_reply_str(&r, "phase", "heaps");
    td_reply_int(&r, "free_internal", heap_caps_get_free_size(MALLOC_CAP_INTERNAL));
    td_reply_int(&r, "min_free_internal", heap_caps_get_minimum_free_size(MALLOC_CAP_INTERNAL));
    display_perf_send(&r);
}

void display_perf_cmd_mem_boot(void)
{
    char *buf = display_perf_alloc_scratch(MEM_REPLY_MAX);
    if (buf == NULL) {
        display_perf_send_error("no memory");
        return;
    }
    td_reply_t r;
    td_reply_begin(&r, buf, MEM_REPLY_MAX, "perf", true);
    td_reply_str(&r, "op", "mem");
    td_reply_str(&r, "phase", "boot");
    td_reply_int(&r, "boot_count", display_perf_boot_count());
    display_perf_reply_printf(&r, ",\"stages\":[");
    for (uint32_t i = 0; i < s_stages_n; i++) {
        const boot_stage_t *st = &s_stages[i];
        display_perf_reply_printf(&r,
                                  "%s{\"stage\":\"%s\",\"ms\":%lu,\"free_internal\":%lu,\"min_free_internal\":%lu,"
                                  "\"largest_internal\":%lu,\"free_psram\":%lu}",
                                  i == 0 ? "" : ",", st->stage, (unsigned long)st->ms, (unsigned long)st->free_internal,
                                  (unsigned long)st->min_free_internal, (unsigned long)st->largest_internal,
                                  (unsigned long)st->free_psram);
    }
    display_perf_reply_printf(&r, "]");
    display_perf_send(&r);
    free(buf);
}

void display_perf_cmd_mem_last(void)
{
    const last_mem_t *l = &s_last;
    const bool intact = l->magic == LAST_MAGIC && l->tasks_n <= LAST_TASKS_MAX && l->crc == last_crc(l);
    /* A record from an older boot, left there when the previous one ended without esp_restart, is not this one's. */
    const bool saved = intact && l->boot + 1 == display_perf_boot_count();
    char *buf = display_perf_alloc_scratch(MEM_REPLY_MAX);
    if (buf == NULL) {
        display_perf_send_error("no memory");
        return;
    }
    td_reply_t r;
    td_reply_begin(&r, buf, MEM_REPLY_MAX, "perf", true);
    td_reply_str(&r, "op", "mem");
    td_reply_str(&r, "phase", "last");
    td_reply_bool(&r, "saved", saved);
    td_reply_int(&r, "boot_count", display_perf_boot_count());
    if (intact && !saved) {
        td_reply_int(&r, "stale_boot", l->boot);
    }
    if (saved) {
        td_reply_int(&r, "boot", l->boot);
        td_reply_int(&r, "uptime_ms", l->uptime_ms);
        td_reply_int(&r, "free_internal", l->free_internal);
        td_reply_int(&r, "min_free_internal", l->min_free_internal);
        td_reply_int(&r, "free_psram", l->free_psram);
        td_reply_int(&r, "min_free_psram", l->min_free_psram);
        td_reply_int(&r, "free_dma", l->free_dma);
        td_reply_int(&r, "min_free_dma", l->min_free_dma);
        display_perf_reply_printf(&r, ",\"tasks\":[");
        for (uint32_t i = 0; i < l->tasks_n; i++) {
            char name[configMAX_TASK_NAME_LEN + 1];
            memcpy(name, l->tasks[i].name, configMAX_TASK_NAME_LEN);
            name[configMAX_TASK_NAME_LEN] = '\0';
            display_perf_reply_printf(&r, "%s", i == 0 ? "" : ",");
            display_perf_reply_task_name(&r, name);
            display_perf_reply_printf(&r, ",\"hwm\":%lu}", (unsigned long)l->tasks[i].hwm);
        }
        display_perf_reply_printf(&r, "]");
    }
    display_perf_send(&r);
    free(buf);
}

/* The word a pattern puts at an address: four fixed fills, then each word's own address, then its complement. */
static uint32_t memtest_word(int pattern, const volatile uint32_t *at)
{
    static const uint32_t fills[] = { 0x00000000u, 0xffffffffu, 0xaaaaaaaau, 0x55555555u };
    if (pattern < 4) {
        return fills[pattern];
    }
    const uint32_t address = (uint32_t)(uintptr_t)at;
    return pattern == 4 ? address : ~address;
}

/* Writes each pattern over a PSRAM block of its own, writes the cache back and drops it, so the reads come from
 * PSRAM, and counts the words that read back wrong. It touches nothing but that block. */
void display_perf_cmd_memtest(int argc, char **argv)
{
    uint32_t kb = 0;
    if (argc != 3 || !display_perf_parse_uint(argv[2], 1, MEMTEST_MAX_KB, &kb)) {
        display_perf_send_error("usage: perf memtest <kb 1-4096>");
        return;
    }
    const size_t bytes = (size_t)kb * 1024;
    volatile uint32_t *words = heap_caps_aligned_alloc(MEMTEST_ALIGN, bytes, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
    if (words == NULL) {
        display_perf_send_error("no memory");
        return;
    }

    const size_t n = bytes / sizeof *words;
    uint32_t errors = 0;
    int64_t first_error = -1;
    const int64_t t0 = esp_timer_get_time();
    for (int pattern = 0; pattern < MEMTEST_PATTERNS; pattern++) {
        for (size_t i = 0; i < n; i++) {
            words[i] = memtest_word(pattern, &words[i]);
        }
        ESP_ERROR_CHECK(esp_cache_msync((void *)words, bytes, ESP_CACHE_MSYNC_FLAG_DIR_C2M |
                                                                ESP_CACHE_MSYNC_FLAG_INVALIDATE));
        for (size_t i = 0; i < n; i++) {
            if (words[i] != memtest_word(pattern, &words[i])) {
                errors++;
                first_error = first_error < 0 ? (int64_t)(i * sizeof *words) : first_error;
            }
        }
        /* A few hundred milliseconds a pattern at most, then the idle task gets its turn. */
        vTaskDelay(1);
    }
    const int64_t us = esp_timer_get_time() - t0;
    heap_caps_free((void *)words);

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "perf", errors == 0);
    td_reply_str(&r, "op", "memtest");
    td_reply_int(&r, "kb", kb);
    td_reply_int(&r, "bytes", (long long)bytes);
    td_reply_int(&r, "patterns", MEMTEST_PATTERNS);
    td_reply_int(&r, "errors", errors);
    if (first_error >= 0) {
        td_reply_int(&r, "first_error_offset", first_error);
    } else {
        display_perf_reply_null(&r, "first_error_offset");
    }
    td_reply_int(&r, "us", us);
    display_perf_send(&r);
}

typedef enum {
    MEMBW_SRAM_TO_PSRAM,
    MEMBW_PSRAM_TO_SRAM,
    MEMBW_PSRAM_TO_PSRAM,
    MEMBW_DIRECTIONS,
} membw_dir_t;

typedef struct {
    uint8_t *sram;
    uint8_t *src;
    uint8_t *dst;
    TaskHandle_t waiter;
    int core;
    uint32_t us[MEMBW_DIRECTIONS];
} membw_job_t;

static void psram_drop(void *p)
{
    ESP_ERROR_CHECK(esp_cache_msync(p, MEMBW_PSRAM_BYTES, ESP_CACHE_MSYNC_FLAG_DIR_C2M |
                                                           ESP_CACHE_MSYNC_FLAG_INVALIDATE));
}

static void psram_write_back(void *p)
{
    ESP_ERROR_CHECK(esp_cache_msync(p, MEMBW_PSRAM_BYTES, ESP_CACHE_MSYNC_FLAG_DIR_C2M));
}

/*
 * How the copies are timed. Before each pass the PSRAM blocks are written back and dropped from the data cache,
 * outside the timed span, so the copy's PSRAM reads miss the cache and go to the bus. After a copy into PSRAM the
 * destination is written back inside the timed span, so every dirty line has reached PSRAM when the clock stops.
 * Each PSRAM block is 512 KB, eight times the 64 KB data cache, and an SRAM side goes 4 KB at a time through one
 * internal buffer. The panel keeps scanning out meanwhile, so its bounce buffer copy shares the bus as it does in
 * use. The task runs on core 1, away from the panel interrupt on core 0, so interrupt time does not stretch the
 * figures.
 */
static uint32_t membw_pass(membw_job_t *j, membw_dir_t dir)
{
    psram_drop(j->src);
    psram_drop(j->dst);
    const int64_t t0 = esp_timer_get_time();
    switch (dir) {
    case MEMBW_SRAM_TO_PSRAM:
        for (size_t off = 0; off < MEMBW_PSRAM_BYTES; off += MEMBW_SRAM_BYTES) {
            memcpy(j->dst + off, j->sram, MEMBW_SRAM_BYTES);
        }
        psram_write_back(j->dst);
        break;
    case MEMBW_PSRAM_TO_SRAM:
        for (size_t off = 0; off < MEMBW_PSRAM_BYTES; off += MEMBW_SRAM_BYTES) {
            memcpy(j->sram, j->src + off, MEMBW_SRAM_BYTES);
        }
        break;
    default:
        memcpy(j->dst, j->src, MEMBW_PSRAM_BYTES);
        psram_write_back(j->dst);
        break;
    }
    return (uint32_t)(esp_timer_get_time() - t0);
}

static void membw_task(void *arg)
{
    membw_job_t *j = arg;
    j->core = xPortGetCoreID();
    for (int dir = 0; dir < MEMBW_DIRECTIONS; dir++) {
        for (int pass = 0; pass < MEMBW_PASSES; pass++) {
            j->us[dir] += membw_pass(j, (membw_dir_t)dir);
        }
    }
    xTaskNotifyGive(j->waiter);
    vTaskSuspend(NULL);
}

static void reply_mbps(td_reply_t *r, const char *key, uint32_t us)
{
    const uint64_t bytes = (uint64_t)MEMBW_PSRAM_BYTES * MEMBW_PASSES;
    display_perf_reply_tenths(r, key, us ? (int32_t)(bytes * 10 / us) : -1);
}

void display_perf_cmd_membw(void)
{
    membw_job_t job = { .waiter = xTaskGetCurrentTaskHandle() };
    job.sram = heap_caps_aligned_alloc(MEMBW_ALIGN, MEMBW_SRAM_BYTES, MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT);
    job.src = heap_caps_aligned_alloc(MEMBW_ALIGN, MEMBW_PSRAM_BYTES, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
    job.dst = heap_caps_aligned_alloc(MEMBW_ALIGN, MEMBW_PSRAM_BYTES, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
    TaskHandle_t task = NULL;
    if (job.sram != NULL && job.src != NULL && job.dst != NULL) {
        memset(job.src, 0x5a, MEMBW_PSRAM_BYTES);
        memset(job.sram, 0xa5, MEMBW_SRAM_BYTES);
    }
    if (job.sram == NULL || job.src == NULL || job.dst == NULL) {
        display_perf_send_error("no memory");
    } else if (xTaskCreatePinnedToCoreWithCaps(membw_task, "td_perf_bw", MEMBW_STACK, &job, MEMBW_PRIORITY, &task,
                                               MEMBW_CORE, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT) != pdPASS) {
        display_perf_send_error("cannot start the perf task");
    } else {
        (void)ulTaskNotifyTake(pdTRUE, portMAX_DELAY);
        while (eTaskGetState(task) != eSuspended) {
            vTaskDelay(1);
        }
        vTaskDeleteWithCaps(task);

        char buf[TD_REPLY_MAX];
        td_reply_t r;
        td_reply_begin(&r, buf, sizeof buf, "perf", true);
        td_reply_str(&r, "op", "membw");
        td_reply_int(&r, "core", job.core);
        td_reply_int(&r, "bytes", (long long)MEMBW_PSRAM_BYTES * MEMBW_PASSES);
        reply_mbps(&r, "sram_to_psram_mbps", job.us[MEMBW_SRAM_TO_PSRAM]);
        reply_mbps(&r, "psram_to_sram_mbps", job.us[MEMBW_PSRAM_TO_SRAM]);
        reply_mbps(&r, "psram_to_psram_mbps", job.us[MEMBW_PSRAM_TO_PSRAM]);
        display_perf_send(&r);
    }
    heap_caps_free(job.dst);
    heap_caps_free(job.src);
    heap_caps_free(job.sram);
}
