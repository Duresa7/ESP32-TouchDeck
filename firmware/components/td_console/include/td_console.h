#pragma once

#include <stdbool.h>
#include <stddef.h>
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

esp_err_t td_console_start(void);

#ifdef __cplusplus
}
#endif
