#pragma once

#include <stdbool.h>
#include <stddef.h>
#include "esp_console.h"
#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif

#define TD_CMD_TABLE_MAX 32

/*
 * The console's commands. `help` lists this table and the console registers it with esp_console in one go, so
 * the two cannot disagree. Nothing is added once the table is sealed, which keeps esp_console's unlocked command
 * list single-writer: only td_console_start touches it, before any task reads it.
 */
typedef struct {
    esp_console_cmd_t cmds[TD_CMD_TABLE_MAX];
    size_t count;
    bool sealed;
} td_cmd_table_t;

/*
 * Appends count commands, or none when any is refused. The strings must outlive the table, as esp_console
 * requires. ESP_ERR_INVALID_STATE once sealed, ESP_ERR_INVALID_ARG for a missing or spaced name, for func and
 * func_w_context both set or both clear, and for a name already in the table (esp_console would replace the
 * first silently, and `help` would list it twice), ESP_ERR_NO_MEM when the table is full.
 */
esp_err_t td_cmd_table_add(td_cmd_table_t *t, const esp_console_cmd_t *cmds, size_t count);

void td_cmd_table_seal(td_cmd_table_t *t);

/* Copies the command names into names, which holds TD_CMD_TABLE_MAX entries, and returns how many. */
size_t td_cmd_table_names(const td_cmd_table_t *t, const char **names);

#ifdef __cplusplus
}
#endif
