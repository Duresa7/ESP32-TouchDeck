#include "display_priv.h"
#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>
#include "board_pins.h"
#include "ch422g.h"
#include "esp_console.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "td_console.h"

#define PERF_MAX_SECONDS 60
#define PERF_TASK_STACK 4096
#define PERF_TASK_PRIORITY 2

static atomic_bool s_perf_busy;

static void send(td_reply_t *r)
{
    td_reply_end(r);
    td_console_write_line(r->buf);
}

static void send_error(const char *cmd, const char *message)
{
    char buf[TD_REPLY_MAX];
    td_reply_error(buf, sizeof buf, cmd, message);
    td_console_write_line(buf);
}

/* An image line must not lose bytes, and the stdio path would drop it while the display runs. */
static bool stdout_sink(void *ctx, const char *line, size_t len)
{
    (void)ctx;
    return td_console_write(line, len);
}

/* The console starts before the display does, so a command can arrive while the display is still coming up, or
 * after it failed to. The screenshot and perf paths need the LVGL adapter, which only exists once it has started. */
static bool require_display(const char *cmd)
{
    if (display_ready()) {
        return true;
    }
    send_error(cmd, "display not ready");
    return false;
}

static int cmd_screenshot(int argc, char **argv)
{
    td_shot_scale_t scale;
    if (argc > 2 || !td_shot_parse_scale(argc > 1 ? argv[1] : NULL, &scale)) {
        send_error("screenshot", "usage: screenshot [1|2|4]");
        return 0;
    }
    if (!require_display("screenshot")) {
        return 0;
    }

    td_shot_image_t img;
    esp_err_t err = display_snapshot(&img);
    if (err != ESP_OK) {
        send_error("screenshot", err == ESP_ERR_INVALID_STATE ? "busy" :
                                 err == ESP_ERR_TIMEOUT ? "display busy" : "snapshot failed");
        return 0;
    }

    const td_shot_layout_t layout = td_shot_layout(&img, scale);
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "screenshot", true);
    td_reply_str(&r, "phase", "start");
    td_reply_int(&r, "scale", scale);
    td_reply_int(&r, "width", layout.width);
    td_reply_int(&r, "height", layout.height);
    td_reply_str(&r, "format", "rgb565");
    td_reply_int(&r, "bytes", layout.bytes);
    td_reply_int(&r, "lines", layout.lines);
    send(&r);

    uint32_t crc = 0;
    err = td_shot_stream(&img, scale, stdout_sink, NULL, &crc);

    td_reply_begin(&r, buf, sizeof buf, "screenshot", err == ESP_OK);
    td_reply_str(&r, "phase", "end");
    td_reply_int(&r, "bytes", layout.bytes);
    td_reply_int(&r, "crc32", crc);
    if (err != ESP_OK) {
        td_reply_str(&r, "error", "stream aborted");
    }
    send(&r);
    return 0;
}

static void perf_task(void *arg)
{
    const uint32_t seconds = (uint32_t)(uintptr_t)arg;
    display_perf_t perf;
    esp_err_t err = display_perf_run(seconds, &perf);

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "perf", err == ESP_OK);
    if (err == ESP_OK) {
        td_reply_int(&r, "seconds", perf.seconds);
        td_reply_int(&r, "frames", perf.frames);
        td_reply_int(&r, "elapsed_ms", perf.elapsed_ms);
        td_reply_int(&r, "fps", perf.elapsed_ms ? (uint64_t)perf.frames * 1000 / perf.elapsed_ms : 0);
    } else {
        td_reply_str(&r, "error", esp_err_to_name(err));
    }
    send(&r);

    atomic_store(&s_perf_busy, false);
    vTaskDelete(NULL);
}

/* The reply comes when the run ends, so the console stays free to take screenshots during it. */
static int cmd_perf(int argc, char **argv)
{
    char *end = NULL;
    const unsigned long seconds = argc == 3 ? strtoul(argv[2], &end, 10) : 0;
    if (argc != 3 || strcmp(argv[1], "fps") != 0 || *argv[2] == '\0' || *end != '\0' || seconds < 1 ||
        seconds > PERF_MAX_SECONDS) {
        send_error("perf", "usage: perf fps <seconds 1-60>");
        return 0;
    }
    if (!require_display("perf")) {
        return 0;
    }
    if (atomic_exchange(&s_perf_busy, true)) {
        send_error("perf", "perf already running");
        return 0;
    }
    if (xTaskCreate(perf_task, "td_perf", PERF_TASK_STACK, (void *)(uintptr_t)seconds, PERF_TASK_PRIORITY, NULL) !=
        pdPASS) {
        atomic_store(&s_perf_busy, false);
        send_error("perf", "cannot start the perf task");
    }
    return 0;
}

static int cmd_backlight(int argc, char **argv)
{
    esp_err_t err = ESP_OK;
    if (argc == 2 && strcmp(argv[1], "on") == 0) {
        err = ch422g_set_backlight(true);
    } else if (argc == 2 && strcmp(argv[1], "off") == 0) {
        err = ch422g_set_backlight(false);
    } else if (argc != 1) {
        send_error("backlight", "usage: backlight [on|off]");
        return 0;
    }

    const uint8_t output = ch422g_output();
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "backlight", err == ESP_OK);
    td_reply_bool(&r, "on", (output & TD_CH422G_IO_DISP) != 0);
    td_reply_int(&r, "output", output);
    if (err != ESP_OK) {
        td_reply_str(&r, "error", esp_err_to_name(err));
    }
    send(&r);
    return 0;
}

const esp_console_cmd_t *display_console_commands(size_t *count)
{
    static const esp_console_cmd_t cmds[] = {
        { .command = "screenshot", .help = "stream the screen as RGB565 base64 lines, scale 1, 2 or 4",
          .hint = "[1|2|4]", .func = cmd_screenshot },
        { .command = "perf", .help = "measure frames per second, reply arrives when done",
          .hint = "fps <seconds>", .func = cmd_perf },
        { .command = "backlight", .help = "turn the backlight on or off, no argument reports it",
          .hint = "[on|off]", .func = cmd_backlight },
    };
    *count = sizeof cmds / sizeof cmds[0];
    return cmds;
}
