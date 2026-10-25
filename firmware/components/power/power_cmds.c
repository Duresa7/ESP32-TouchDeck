#include "power.h"
#include <stdlib.h>
#include <string.h>
#include "esp_timer.h"
#include "td_console.h"

static void send(td_reply_t *r)
{
    td_reply_end(r);
    td_console_write_line(r->buf);
}

static void send_error(const char *message)
{
    char buf[TD_REPLY_MAX];
    td_reply_error(buf, sizeof buf, "power", message);
    td_console_write_line(buf);
}

static void cmd_state(void)
{
    power_info_t info;
    if (power_get_info(&info, esp_timer_get_time()) != ESP_OK) {
        send_error("power not ready");
        return;
    }

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "power", true);
    td_reply_str(&r, "op", "state");
    td_reply_str(&r, "state", info.state == POWER_ACTIVE ? "ACTIVE" : "OFF");
    td_reply_int(&r, "idle_timeout_s", info.idle_timeout_s);
    td_reply_int(&r, "idle_ms", info.idle_ms);
    td_reply_bool(&r, "swallowing", info.swallowing);
    td_reply_int(&r, "swallowed", info.swallowed);
    td_reply_int(&r, "off_count", info.off_count);
    td_reply_int(&r, "wake_count", info.wake_count);
    td_reply_int(&r, "wake_us", info.wake_us);
    td_reply_int(&r, "errors", info.errors);
    td_reply_int(&r, "output", info.output);
    send(&r);
}

/* strtoul accepts a sign and leading space, so the first character must be a digit. A number too large for
 * unsigned long saturates and lands out of range. */
static void cmd_idle_timeout(const char *word)
{
    if (word[0] < '0' || word[0] > '9') {
        send_error("usage: power idle-timeout <seconds 1-86400>");
        return;
    }
    char *end = NULL;
    const unsigned long seconds = strtoul(word, &end, 10);
    if (*end != '\0') {
        send_error("usage: power idle-timeout <seconds 1-86400>");
        return;
    }
    const esp_err_t err = power_set_idle_timeout_s((uint32_t)seconds, esp_timer_get_time());
    if (err != ESP_OK) {
        send_error(err == ESP_ERR_INVALID_ARG ? "out of range" : "power not ready");
        return;
    }

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "power", true);
    td_reply_str(&r, "op", "idle-timeout");
    td_reply_int(&r, "idle_timeout_s", (long long)seconds);
    send(&r);
}

static int cmd_power(int argc, char **argv)
{
    const char *sub = argc > 1 ? argv[1] : "";
    if (strcmp(sub, "state") == 0 && argc == 2) {
        cmd_state();
    } else if (strcmp(sub, "idle-timeout") == 0 && argc == 3) {
        cmd_idle_timeout(argv[2]);
    } else {
        send_error("usage: power state | idle-timeout <seconds 1-86400>");
    }
    return 0;
}

const esp_console_cmd_t *power_console_commands(size_t *count)
{
    static const esp_console_cmd_t cmds[] = {
        { .command = "power", .help = "report the idle screen-off state, or set the idle timeout until the next reset",
          .hint = "state | idle-timeout <seconds>", .func = cmd_power },
    };
    *count = sizeof cmds / sizeof cmds[0];
    return cmds;
}
