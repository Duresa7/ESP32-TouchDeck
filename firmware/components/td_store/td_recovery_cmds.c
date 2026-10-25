#include <string.h>
#include "esp_system.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "td_console.h"
#include "td_recovery.h"
#include "td_recovery_priv.h"

#define ARM_REPLY_FLUSH_MS 200

static void send(td_reply_t *r)
{
    td_reply_end(r);
    td_console_write_line(r->buf);
}

static void send_error(const char *message)
{
    char buf[TD_REPLY_MAX];
    td_reply_error(buf, sizeof buf, "recovery", message);
    td_console_write_line(buf);
}

static void cmd_state(void)
{
    td_recovery_info_t info;
    td_recovery_get_info(&info);

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "recovery", true);
    td_reply_str(&r, "op", "state");
    td_reply_bool(&r, "mode", td_recovery_mode());
    td_reply_str(&r, "watcher", info.watcher);
    td_reply_int(&r, "held_ms", info.held_ms);
    td_reply_int(&r, "armed_count", info.armed_count);
    send(&r);
}

/* A test hook. It does what the hold does after the lift: it writes the marker and restarts. */
static void cmd_arm(void)
{
    if (td_recovery_mark() != ESP_OK) {
        send_error("marker write failed");
        return;
    }
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "recovery", true);
    td_reply_str(&r, "op", "arm");
    send(&r);
    vTaskDelay(pdMS_TO_TICKS(ARM_REPLY_FLUSH_MS));
    esp_restart();
}

static int cmd_recovery(int argc, char **argv)
{
    const char *sub = argc > 1 ? argv[1] : "";
    if (argc == 2 && strcmp(sub, "state") == 0) {
        cmd_state();
    } else if (argc == 2 && strcmp(sub, "arm") == 0) {
        cmd_arm();
    } else {
        send_error("usage: recovery state | arm");
    }
    return 0;
}

const esp_console_cmd_t *td_recovery_console_commands(size_t *count)
{
    static const esp_console_cmd_t cmds[] = {
        { .command = "recovery", .help = "report the recovery hold watcher, or arm recovery and restart",
          .hint = "state | arm", .func = cmd_recovery },
    };
    *count = sizeof cmds / sizeof cmds[0];
    return cmds;
}
