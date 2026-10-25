#include "td_console.h"
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include "esp_app_desc.h"
#include "esp_check.h"
#include "esp_console.h"
#include "esp_log.h"
#include "esp_stdio.h"
#include "esp_system.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#define TD_PROMPT "td> "
#define TD_LINE_MAX 256
#define TD_WORD_MAX 64
#define TD_TASK_STACK 6144
#define TD_TASK_PRIORITY 2

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
    vTaskDelay(pdMS_TO_TICKS(100));
    esp_restart();
    return 0;
}

static const esp_console_cmd_t s_cmds[] = {
    { .command = "help", .help = "list the console commands", .func = cmd_help },
    { .command = "version", .help = "report the app descriptor", .func = cmd_version },
    { .command = "heap", .help = "report free heap", .func = cmd_heap },
    { .command = "uptime", .help = "report milliseconds since boot", .func = cmd_uptime },
    { .command = "reboot", .help = "restart the board", .func = cmd_reboot },
};
#define TD_CMD_COUNT (sizeof s_cmds / sizeof s_cmds[0])

static int cmd_help(int argc, char **argv)
{
    const char *names[TD_CMD_COUNT];
    for (size_t i = 0; i < TD_CMD_COUNT; i++) {
        names[i] = s_cmds[i].command;
    }

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "help", true);
    td_reply_strs(&r, "commands", names, TD_CMD_COUNT);
    emit(&r);
    return 0;
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

static char next_char(rx_t *rx)
{
    while (rx->pos == rx->len) {
        echo_flush(rx);
        ssize_t n = read(STDIN_FILENO, rx->in, sizeof rx->in);
        if (n > 0) {
            rx->pos = 0;
            rx->len = (size_t)n;
        } else {
            vTaskDelay(pdMS_TO_TICKS(10));
        }
    }
    return rx->in[rx->pos++];
}

static bool read_line(rx_t *rx, char *line, size_t cap)
{
    size_t len = 0;
    bool too_long = false;
    for (;;) {
        char c = next_char(rx);
        if (c == '\r' || c == '\n') {
            echo_add(rx, "\n", 1);
            echo_flush(rx);
            line[len] = '\0';
            return !too_long;
        }
        if (c == '\b' || c == 0x7f) {
            if (len > 0 && !too_long) {
                len--;
                echo_add(rx, "\b \b", 3);
            }
        } else if (len + 1 < cap) {
            line[len++] = c;
            echo_add(rx, &c, 1);
        } else {
            too_long = true;
        }
    }
}

static void first_word(const char *line, char *word, size_t cap)
{
    while (*line == ' ' || *line == '\t') {
        line++;
    }
    size_t n = strcspn(line, " \t");
    if (n >= cap) {
        n = cap - 1;
    }
    memcpy(word, line, n);
    word[n] = '\0';
}

static void run_line(const char *line)
{
    int ret;
    esp_err_t err = esp_console_run(line, &ret);
    if (err == ESP_OK || err == ESP_ERR_INVALID_ARG) {
        return;
    }
    char word[TD_WORD_MAX];
    first_word(line, word, sizeof word);
    emit_error(word, err == ESP_ERR_NOT_FOUND ? "unknown command" : "internal error");
}

static void console_task(void *arg)
{
    rx_t rx = { 0 };
    char line[TD_LINE_MAX];
    for (;;) {
        printf(TD_PROMPT);
        fflush(stdout);
        if (read_line(&rx, line, sizeof line)) {
            run_line(line);
        } else {
            char word[TD_WORD_MAX];
            first_word(line, word, sizeof word);
            emit_error(word, "line too long");
        }
    }
}

esp_err_t td_console_start(void)
{
    ESP_RETURN_ON_ERROR(esp_stdio_install_io_driver(), TAG, "stdio driver");

    esp_console_config_t config = ESP_CONSOLE_CONFIG_DEFAULT();
    config.max_cmdline_length = TD_LINE_MAX;
    ESP_RETURN_ON_ERROR(esp_console_init(&config), TAG, "console init");
    for (size_t i = 0; i < TD_CMD_COUNT; i++) {
        ESP_RETURN_ON_ERROR(esp_console_cmd_register(&s_cmds[i]), TAG, "register %s", s_cmds[i].command);
    }

    if (xTaskCreate(console_task, "td_console", TD_TASK_STACK, NULL, TD_TASK_PRIORITY, NULL) != pdPASS) {
        return ESP_FAIL;
    }
    return ESP_OK;
}
