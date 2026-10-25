#include "td_console.h"
#include "td_cmd_table.h"
#include "td_rx_sync.h"
#include <stdatomic.h>
#include <stdio.h>
#include <string.h>
#include "esp_app_desc.h"
#include "esp_check.h"
#include "esp_console.h"
#include "esp_heap_caps.h"
#include "esp_log.h"
#include "esp_stdio.h"
#include "esp_system.h"
#include "esp_timer.h"
#include "driver/usb_serial_jtag.h"
#include "freertos/FreeRTOS.h"
#include "freertos/stream_buffer.h"
#include "freertos/task.h"

#define TD_PROMPT "td> "
#define TD_LINE_MAX 256
#define TD_WORD_MAX 64
#define TD_TASK_STACK 6144
#define TD_TASK_PRIORITY 2
#define TD_RX_TASK_STACK 2048
#define TD_RX_TASK_PRIORITY 10
#define TD_RX_CHUNK 64
#define TD_RX_QUEUE_BYTES 2048
#define TD_RX_IDLE_MS 100
#define TD_TX_DRAIN_MS 250

static const char *TAG = "td_console";

static void emit(td_reply_t *r)
{
    td_reply_end(r);
    printf("%s\n", r->buf);
}

static void emit_error(const char *cmd, const char *message)
{
    char buf[TD_REPLY_MAX];
    td_reply_error(buf, sizeof buf, cmd, message);
    printf("%s\n", buf);
}

static int cmd_help(int argc, char **argv);

static int cmd_version(int argc, char **argv)
{
    const esp_app_desc_t *app = esp_app_get_description();
    char sha[2 * sizeof app->app_elf_sha256 + 1];
    for (size_t i = 0; i < sizeof app->app_elf_sha256; i++) {
        snprintf(sha + 2 * i, 3, "%02x", app->app_elf_sha256[i]);
    }

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "version", true);
    td_reply_str(&r, "project_name", app->project_name);
    td_reply_str(&r, "version", app->version);
    td_reply_str(&r, "idf_ver", app->idf_ver);
    td_reply_str(&r, "date", app->date);
    td_reply_str(&r, "time", app->time);
    td_reply_str(&r, "elf_sha256", sha);
    emit(&r);
    return 0;
}

static int cmd_heap(int argc, char **argv)
{
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "heap", true);
    td_reply_int(&r, "free_heap", esp_get_free_heap_size());
    td_reply_int(&r, "min_free_heap", esp_get_minimum_free_heap_size());
    td_reply_int(&r, "free_internal", heap_caps_get_free_size(MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT));
    td_reply_int(&r, "min_free_internal", heap_caps_get_minimum_free_size(MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT));
    td_reply_int(&r, "free_psram", heap_caps_get_free_size(MALLOC_CAP_SPIRAM));
    td_reply_int(&r, "min_free_psram", heap_caps_get_minimum_free_size(MALLOC_CAP_SPIRAM));
    emit(&r);
    return 0;
}

static int cmd_uptime(int argc, char **argv)
{
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "uptime", true);
    td_reply_int(&r, "uptime_ms", esp_timer_get_time() / 1000);
    emit(&r);
    return 0;
}

static int cmd_reboot(int argc, char **argv)
{
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "reboot", true);
    emit(&r);
    fflush(stdout);
    usb_serial_jtag_wait_tx_done(pdMS_TO_TICKS(TD_TX_DRAIN_MS));
    esp_restart();
    return 0;
}

static const esp_console_cmd_t s_builtin_cmds[] = {
    { .command = "help", .help = "list the console commands", .func = cmd_help },
    { .command = "version", .help = "report the app descriptor", .func = cmd_version },
    { .command = "heap", .help = "report free heap", .func = cmd_heap },
    { .command = "uptime", .help = "report milliseconds since boot", .func = cmd_uptime },
    { .command = "reboot", .help = "restart the board", .func = cmd_reboot },
};

static td_cmd_table_t s_table;

/* The table starts with the console's own commands. `help` lists them first, and no other component can reuse
 * their names. */
static td_cmd_table_t *table(void)
{
    if (s_table.count == 0) {
        td_cmd_table_add(&s_table, s_builtin_cmds, sizeof s_builtin_cmds / sizeof s_builtin_cmds[0]);
    }
    return &s_table;
}

static int cmd_help(int argc, char **argv)
{
    const char *names[TD_CMD_TABLE_MAX];
    const size_t count = td_cmd_table_names(table(), names);

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "help", true);
    td_reply_strs(&r, "commands", names, count);
    emit(&r);
    return 0;
}

esp_err_t td_console_register(const esp_console_cmd_t *cmds, size_t count)
{
    const esp_err_t err = td_cmd_table_add(table(), cmds, count);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "registering %u command(s) starting with \"%s\" failed: %s%s", (unsigned)count,
                 count > 0 && cmds[0].command ? cmds[0].command : "", esp_err_to_name(err),
                 err == ESP_ERR_INVALID_STATE ? " (register before td_console_start)" : "");
    }
    return err;
}

typedef struct {
    char in[64];
    size_t pos;
    size_t len;
    char echo[3 * 64];
    size_t echo_len;
} rx_t;

static void echo_flush(rx_t *rx)
{
    if (rx->echo_len > 0) {
        fwrite(rx->echo, 1, rx->echo_len, stdout);
        fflush(stdout);
        rx->echo_len = 0;
    }
}

static void echo_add(rx_t *rx, const char *s, size_t n)
{
    memcpy(rx->echo + rx->echo_len, s, n);
    rx->echo_len += n;
}

static StreamBufferHandle_t s_rx_queue;
static atomic_uint s_rx_dropped;

/*
 * The driver's RX interrupt copies each USB packet into a 256-byte ring and
 * discards the whole packet, without a trace, when the ring is full. Printing
 * costs the console task about 13 us per output byte, so a 220-byte reply
 * keeps it from reading for about 3 ms and a burst of short lines fills that
 * ring. This task outranks the console and drains the ring into s_rx_queue in
 * reads of up to 64 bytes. When the queue is full, td_rx_sync drops the read and
 * the rest of the line it cuts, and the loss is counted and reported instead of
 * hidden. It reads the driver directly because read() on stdin takes one byte
 * per ring operation, which is slower than back-to-back 64-byte packets.
 */
static void rx_task(void *arg)
{
    td_rx_sync_t sync = { 0 };
    char chunk[TD_RX_CHUNK];
    for (;;) {
        int n = usb_serial_jtag_read_bytes(chunk, sizeof chunk, portMAX_DELAY);
        if (n <= 0) {
            vTaskDelay(pdMS_TO_TICKS(10));
            continue;
        }
        size_t lost = 0;
        size_t m = td_rx_sync(&sync, chunk, (size_t)n, xStreamBufferSpacesAvailable(s_rx_queue), &lost);
        if (m > 0) {
            /* td_rx_sync leaves one byte free, so a short send means that guarantee broke. Count what did not fit. */
            lost += m - xStreamBufferSend(s_rx_queue, chunk, m, 0);
        }
        if (lost > 0) {
            atomic_fetch_add(&s_rx_dropped, (unsigned)lost);
        }
    }
}

static void report_dropped_input(void)
{
    unsigned dropped = atomic_exchange(&s_rx_dropped, 0);
    if (dropped == 0) {
        return;
    }
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "console", false);
    td_reply_str(&r, "error", "input overflow");
    td_reply_int(&r, "dropped_bytes", dropped);
    emit(&r);
}

typedef enum {
    LINE_OK,
    LINE_TOO_LONG,
    LINE_RESTART,
} line_status_t;

static int next_char(rx_t *rx)
{
    while (rx->pos == rx->len) {
        echo_flush(rx);
        rx->len = xStreamBufferReceive(s_rx_queue, rx->in, sizeof rx->in, pdMS_TO_TICKS(TD_RX_IDLE_MS));
        rx->pos = 0;
        if (rx->len == 0) {
            return -1;
        }
    }
    return (unsigned char)rx->in[rx->pos++];
}

static line_status_t restart_line(rx_t *rx)
{
    echo_add(rx, "\n", 1);
    echo_flush(rx);
    return LINE_RESTART;
}

/*
 * LINE_RESTART makes the caller report dropped input and print a fresh prompt.
 * It comes when td_rx_sync abandons the line being read, and when the console
 * has sat idle with a dropped-input count nobody has printed yet.
 */
static line_status_t read_line(rx_t *rx, char *line, size_t cap)
{
    size_t len = 0;
    bool too_long = false;
    for (;;) {
        int c = next_char(rx);
        if (c < 0) {
            if (len == 0 && atomic_load(&s_rx_dropped) > 0) {
                return restart_line(rx);
            }
        } else if (c == TD_RX_ABANDON) {
            return restart_line(rx);
        } else if (c == '\r' || c == '\n') {
            echo_add(rx, "\n", 1);
            echo_flush(rx);
            line[len] = '\0';
            return too_long ? LINE_TOO_LONG : LINE_OK;
        } else if (c == '\b' || c == 0x7f) {
            if (len > 0 && !too_long) {
                len--;
                echo_add(rx, "\b \b", 3);
            }
        } else if (len + 1 < cap) {
            char ch = (char)c;
            line[len++] = ch;
            echo_add(rx, &ch, 1);
        } else {
            too_long = true;
        }
    }
}

static void command_word(const char *line, char *word, size_t cap)
{
    char copy[TD_LINE_MAX];
    char *argv[2];
    strlcpy(copy, line, sizeof copy);
    word[0] = '\0';
    if (esp_console_split_argv(copy, argv, 2) > 0) {
        strlcpy(word, argv[0], cap);
    }
}

static void run_line(const char *line)
{
    int ret;
    esp_err_t err = esp_console_run(line, &ret);
    if (err == ESP_OK || err == ESP_ERR_INVALID_ARG) {
        return;
    }
    char word[TD_WORD_MAX];
    command_word(line, word, sizeof word);
    emit_error(word, err == ESP_ERR_NOT_FOUND ? "unknown command" : "internal error");
}

static void console_task(void *arg)
{
    rx_t rx = { 0 };
    char line[TD_LINE_MAX];
    for (;;) {
        report_dropped_input();
        printf(TD_PROMPT);
        fflush(stdout);
        switch (read_line(&rx, line, sizeof line)) {
        case LINE_OK:
            run_line(line);
            break;
        case LINE_TOO_LONG: {
            char word[TD_WORD_MAX];
            command_word(line, word, sizeof word);
            emit_error(word, "line too long");
            break;
        }
        case LINE_RESTART:
            break;
        }
    }
}

esp_err_t td_console_start(void)
{
    td_cmd_table_seal(table());
    ESP_RETURN_ON_ERROR(esp_stdio_install_io_driver(), TAG, "stdio driver");
    if (!usb_serial_jtag_is_driver_installed()) {
        ESP_LOGW(TAG, "console is off: its input task reads the USB-Serial/JTAG driver, which only the USB-Serial/JTAG primary console installs");
        return ESP_ERR_NOT_SUPPORTED;
    }

    esp_console_config_t config = ESP_CONSOLE_CONFIG_DEFAULT();
    config.max_cmdline_length = TD_LINE_MAX;
    ESP_RETURN_ON_ERROR(esp_console_init(&config), TAG, "console init");
    for (size_t i = 0; i < s_table.count; i++) {
        ESP_RETURN_ON_ERROR(esp_console_cmd_register(&s_table.cmds[i]), TAG, "register %s", s_table.cmds[i].command);
    }

    s_rx_queue = xStreamBufferCreate(TD_RX_QUEUE_BYTES, 1);
    ESP_RETURN_ON_FALSE(s_rx_queue, ESP_ERR_NO_MEM, TAG, "rx queue");
    ESP_RETURN_ON_FALSE(xTaskCreate(rx_task, "td_rx", TD_RX_TASK_STACK, NULL, TD_RX_TASK_PRIORITY, NULL) == pdPASS,
                        ESP_FAIL, TAG, "rx task");
    ESP_RETURN_ON_FALSE(xTaskCreate(console_task, "td_console", TD_TASK_STACK, NULL, TD_TASK_PRIORITY, NULL) == pdPASS,
                        ESP_FAIL, TAG, "console task");
    return ESP_OK;
}
