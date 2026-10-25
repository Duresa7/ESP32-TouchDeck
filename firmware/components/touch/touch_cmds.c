#include "touch.h"
#include "touch_priv.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "board_pins.h"
#include "esp_timer.h"
#include "td_console.h"

#define TAP_TIMEOUT_MS 1000
#define SWIPE_SLACK_MS 1000

typedef enum {
    ARG_OK,
    ARG_BAD,
    ARG_RANGE,
} arg_t;

static void send(td_reply_t *r)
{
    td_reply_end(r);
    printf("%s\n", r->buf);
    fflush(stdout);
}

static void send_error(const char *message)
{
    char buf[TD_REPLY_MAX];
    td_reply_error(buf, sizeof buf, "touch", message);
    printf("%s\n", buf);
    fflush(stdout);
}

static bool require_touch(void)
{
    if (touch_ready()) {
        return true;
    }
    send_error("touch not ready");
    return false;
}

/* strtoul accepts a sign and leading space, so the first character must be a digit. A number too large for
 * unsigned long saturates and lands out of range. */
static arg_t parse_args(char **words, const uint32_t *min, const uint32_t *max, uint32_t *out, size_t n)
{
    arg_t result = ARG_OK;
    for (size_t i = 0; i < n; i++) {
        if (words[i][0] < '0' || words[i][0] > '9') {
            return ARG_BAD;
        }
        char *end = NULL;
        const unsigned long value = strtoul(words[i], &end, 10);
        if (*end != '\0') {
            return ARG_BAD;
        }
        if (value < min[i] || value > max[i]) {
            result = ARG_RANGE;
        }
        out[i] = (uint32_t)value;
    }
    return result;
}

static const char *injection_error(esp_err_t err)
{
    switch (err) {
    case ESP_ERR_INVALID_STATE:
        return "finger down";
    case ESP_ERR_NO_MEM:
        return "queue full";
    case ESP_ERR_TIMEOUT:
        return "not consumed in time";
    default:
        return "inject failed";
    }
}

static bool check_args(arg_t parsed, const char *usage)
{
    if (parsed == ARG_BAD) {
        send_error(usage);
    } else if (parsed == ARG_RANGE) {
        send_error("out of range");
    }
    return parsed == ARG_OK;
}

static void cmd_tap(int argc, char **argv)
{
    static const uint32_t min[] = { 0, 0 };
    static const uint32_t max[] = { TD_LCD_H_RES - 1, TD_LCD_V_RES - 1 };
    uint32_t v[2];
    if (!check_args(argc == 4 ? parse_args(argv + 2, min, max, v, 2) : ARG_BAD, "usage: touch tap <x> <y>") ||
        !require_touch()) {
        return;
    }

    td_touch_sample_t samples[2];
    const size_t n = td_touch_tap((uint16_t)v[0], (uint16_t)v[1], samples);
    uint32_t deliver_us = 0;
    const esp_err_t err = touch_inject(samples, n, TAP_TIMEOUT_MS, &deliver_us);
    if (err != ESP_OK) {
        send_error(injection_error(err));
        return;
    }

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "touch", true);
    td_reply_str(&r, "op", "tap");
    td_reply_int(&r, "x", v[0]);
    td_reply_int(&r, "y", v[1]);
    td_reply_int(&r, "samples", (long long)n);
    td_reply_int(&r, "deliver_us", deliver_us);
    send(&r);
}

static void cmd_swipe(int argc, char **argv)
{
    static const uint32_t min[] = { 0, 0, 0, 0, 1 };
    static const uint32_t max[] = { TD_LCD_H_RES - 1, TD_LCD_V_RES - 1, TD_LCD_H_RES - 1, TD_LCD_V_RES - 1,
                                    TD_TOUCH_SWIPE_MAX_MS };
    uint32_t v[5];
    if (!check_args(argc == 7 ? parse_args(argv + 2, min, max, v, 5) : ARG_BAD,
                    "usage: touch swipe <x1> <y1> <x2> <y2> <ms>") ||
        !require_touch()) {
        return;
    }

    /* Static, because the console task's stack is small. The console runs one command at a time. */
    static td_touch_sample_t samples[TD_TOUCH_QUEUE_CAP];
    const size_t n = td_touch_swipe((uint16_t)v[0], (uint16_t)v[1], (uint16_t)v[2], (uint16_t)v[3], v[4], samples,
                                    TD_TOUCH_QUEUE_CAP);
    uint32_t deliver_us = 0;
    const esp_err_t err = touch_inject(samples, n, v[4] + SWIPE_SLACK_MS, &deliver_us);
    if (err != ESP_OK) {
        send_error(injection_error(err));
        return;
    }

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "touch", true);
    td_reply_str(&r, "op", "swipe");
    td_reply_int(&r, "samples", (long long)n);
    td_reply_int(&r, "deliver_us", deliver_us);
    send(&r);
}

static void cmd_log(int argc, char **argv)
{
    const bool on = argc == 3 && strcmp(argv[2], "on") == 0;
    if (!on && !(argc == 3 && strcmp(argv[2], "off") == 0)) {
        send_error("usage: touch log on|off");
        return;
    }
    if (!require_touch()) {
        return;
    }

    touch_set_log(on);
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "touch", true);
    td_reply_str(&r, "op", "log");
    td_reply_bool(&r, "on", on);
    send(&r);
}

static void cmd_stats(int argc)
{
    if (argc != 2) {
        send_error("usage: touch stats");
        return;
    }
    if (!require_touch()) {
        return;
    }

    touch_stats_t st;
    touch_get_stats(&st);
    char address[8];
    snprintf(address, sizeof address, "0x%02X", st.address);

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "touch", true);
    td_reply_str(&r, "op", "stats");
    td_reply_str(&r, "address", address);
    td_reply_int(&r, "polls", st.polls);
    td_reply_int(&r, "frames", st.frames);
    td_reply_int(&r, "pressed_frames", st.pressed_frames);
    td_reply_int(&r, "errors", st.errors);
    td_reply_int(&r, "injected", st.injected);
    td_reply_int(&r, "rejected", st.rejected);
    td_reply_int(&r, "queued", st.queued);
    td_reply_int(&r, "x", st.last.x);
    td_reply_int(&r, "y", st.last.y);
    td_reply_bool(&r, "pressed", st.last.pressed);
    td_reply_int(&r, "contact_frames", st.contact_frames);
    td_reply_int(&r, "contact_ms", st.contact_ms);
    td_reply_int(&r, "contact_hz", st.contact_hz);
    td_reply_int(&r, "uptime_ms", esp_timer_get_time() / 1000);
    send(&r);
}

static int cmd_touch(int argc, char **argv)
{
    const char *sub = argc > 1 ? argv[1] : "";
    if (strcmp(sub, "tap") == 0) {
        cmd_tap(argc, argv);
    } else if (strcmp(sub, "swipe") == 0) {
        cmd_swipe(argc, argv);
    } else if (strcmp(sub, "log") == 0) {
        cmd_log(argc, argv);
    } else if (strcmp(sub, "stats") == 0) {
        cmd_stats(argc);
    } else {
        send_error("usage: touch tap <x> <y> | swipe <x1> <y1> <x2> <y2> <ms> | log on|off | stats");
    }
    return 0;
}

const esp_console_cmd_t *touch_console_commands(size_t *count)
{
    static const esp_console_cmd_t cmds[] = {
        { .command = "touch", .help = "inject taps and swipes, log real touches, report touch statistics",
          .hint = "tap <x> <y> | swipe <x1> <y1> <x2> <y2> <ms> | log on|off | stats", .func = cmd_touch },
    };
    *count = sizeof cmds / sizeof cmds[0];
    return cmds;
}
