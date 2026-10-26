#include "td_netlink_service.h"
#include "td_netlink_priv.h"
#include <string.h>

#define USAGE "usage: net state | forget | pair"

static void write_reply(cJSON *reply)
{
    char line[TD_REPLY_MAX] = "@td ";
    if (!cJSON_PrintPreallocated(reply, line + 4, sizeof line - 4, false)) {
        td_reply_error(line, sizeof line, "net", "reply too long");
    }
    td_console_write_line(line);
}

static int cmd_net(int argc, char **argv)
{
    const char *op = argc == 2 ? argv[1] : "";
    const char *error = NULL;
    if (strcmp(op, "forget") == 0) {
        if (!td_netlink_forget_all(false)) {
            error = "the pairings could not be erased";
        }
    } else if (strcmp(op, "pair") == 0) {
        error = td_netlink_pair_window();
    } else if (strcmp(op, "state") != 0) {
        char line[TD_REPLY_MAX];
        td_reply_error(line, sizeof line, "net", USAGE);
        td_console_write_line(line);
        return 1;
    }
    cJSON *reply = cJSON_CreateObject();
    if (!reply) {
        return 1;
    }
    cJSON_AddStringToObject(reply, "cmd", "net");
    cJSON_AddBoolToObject(reply, "ok", error == NULL);
    cJSON_AddStringToObject(reply, "op", op);
    if (error) {
        cJSON_AddStringToObject(reply, "error", error);
    }
    td_netlink_state_fields(reply);
    write_reply(reply);
    cJSON_Delete(reply);
    return error ? 1 : 0;
}

const esp_console_cmd_t *td_netlink_console_commands(size_t *count)
{
    static const esp_console_cmd_t commands[] = {
        { .command = "net", .help = "net state | forget | pair: the Wi-Fi app link and its pairings", .func = cmd_net },
    };
    *count = sizeof commands / sizeof commands[0];
    return commands;
}
