#pragma once

#include <stdbool.h>
#include <stddef.h>
#include "esp_console.h"
#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif

#define TD_REPLY_MAX 512

typedef struct {
    char *buf;
    size_t cap;
    size_t len;
    const char *cmd;
    bool overflow;
} td_reply_t;

void td_reply_begin(td_reply_t *r, char *buf, size_t cap, const char *cmd, bool ok);
void td_reply_str(td_reply_t *r, const char *key, const char *value);
void td_reply_int(td_reply_t *r, const char *key, long long value);
void td_reply_bool(td_reply_t *r, const char *key, bool value);
void td_reply_strs(td_reply_t *r, const char *key, const char *const *values, size_t count);
bool td_reply_end(td_reply_t *r);
bool td_reply_error(char *buf, size_t cap, const char *cmd, const char *message);

/*
 * Adds commands to the console, next to its own, so `help` lists them. The console stores the entries and runs
 * them, nothing more. Call it before td_console_start, which seals the list. A later call fails with
 * ESP_ERR_INVALID_STATE, logs an error and adds nothing. td_cmd_table_add lists the other errors. The strings
 * must stay valid for good.
 */
esp_err_t td_console_register(const esp_console_cmd_t *cmds, size_t count);

esp_err_t td_console_start(void);

#ifdef __cplusplus
}
#endif
