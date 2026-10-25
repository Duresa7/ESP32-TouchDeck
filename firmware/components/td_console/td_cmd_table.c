#include "td_cmd_table.h"
#include <string.h>

static bool valid(const esp_console_cmd_t *c)
{
    return c->command != NULL && c->command[0] != '\0' && strchr(c->command, ' ') == NULL &&
           (c->func != NULL) != (c->func_w_context != NULL);
}

static bool has(const td_cmd_table_t *t, const char *name)
{
    for (size_t i = 0; i < t->count; i++) {
        if (strcmp(t->cmds[i].command, name) == 0) {
            return true;
        }
    }
    return false;
}

esp_err_t td_cmd_table_add(td_cmd_table_t *t, const esp_console_cmd_t *cmds, size_t count)
{
    if (t->sealed) {
        return ESP_ERR_INVALID_STATE;
    }
    const size_t before = t->count;
    esp_err_t err = ESP_OK;
    for (size_t i = 0; i < count && err == ESP_OK; i++) {
        if (!valid(&cmds[i]) || has(t, cmds[i].command)) {
            err = ESP_ERR_INVALID_ARG;
        } else if (t->count == TD_CMD_TABLE_MAX) {
            err = ESP_ERR_NO_MEM;
        } else {
            t->cmds[t->count++] = cmds[i];
        }
    }
    if (err != ESP_OK) {
        t->count = before;
    }
    return err;
}

void td_cmd_table_seal(td_cmd_table_t *t)
{
    t->sealed = true;
}

size_t td_cmd_table_names(const td_cmd_table_t *t, const char **names)
{
    for (size_t i = 0; i < t->count; i++) {
        names[i] = t->cmds[i].command;
    }
    return t->count;
}
