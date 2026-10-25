#include "td_console.h"
#include <stdio.h>
#include <string.h>

#define TD_REPLY_PREFIX "@td "

static void put(td_reply_t *r, const char *s, size_t n)
{
    if (r->overflow) {
        return;
    }
    if (r->len + n >= r->cap) {
        r->overflow = true;
        return;
    }
    memcpy(r->buf + r->len, s, n);
    r->len += n;
    r->buf[r->len] = '\0';
}

static void put_cstr(td_reply_t *r, const char *s)
{
    put(r, s, strlen(s));
}

static void put_json_string(td_reply_t *r, const char *s)
{
    put(r, "\"", 1);
    for (; *s; s++) {
        unsigned char c = (unsigned char)*s;
        char esc[7];
        switch (c) {
        case '"':
            put(r, "\\\"", 2);
            break;
        case '\\':
            put(r, "\\\\", 2);
            break;
        case '\n':
            put(r, "\\n", 2);
            break;
        case '\r':
            put(r, "\\r", 2);
            break;
        case '\t':
            put(r, "\\t", 2);
            break;
        default:
            if (c < 0x20) {
                snprintf(esc, sizeof esc, "\\u%04x", c);
                put(r, esc, 6);
            } else {
                put(r, s, 1);
            }
        }
    }
    put(r, "\"", 1);
}

static void put_key(td_reply_t *r, const char *key)
{
    put(r, ",", 1);
    put_json_string(r, key);
    put(r, ":", 1);
}

void td_reply_begin(td_reply_t *r, char *buf, size_t cap, const char *cmd, bool ok)
{
    r->buf = buf;
    r->cap = cap;
    r->len = 0;
    r->cmd = cmd;
    r->overflow = false;
    if (cap > 0) {
        buf[0] = '\0';
    }
    put_cstr(r, TD_REPLY_PREFIX "{\"cmd\":");
    put_json_string(r, cmd);
    put_cstr(r, ok ? ",\"ok\":true" : ",\"ok\":false");
}

void td_reply_str(td_reply_t *r, const char *key, const char *value)
{
    put_key(r, key);
    put_json_string(r, value);
}

void td_reply_int(td_reply_t *r, const char *key, long long value)
{
    char num[24];
    int n = snprintf(num, sizeof num, "%lld", value);
    put_key(r, key);
    put(r, num, (size_t)n);
}

void td_reply_bool(td_reply_t *r, const char *key, bool value)
{
    put_key(r, key);
    put_cstr(r, value ? "true" : "false");
}

void td_reply_strs(td_reply_t *r, const char *key, const char *const *values, size_t count)
{
    put_key(r, key);
    put(r, "[", 1);
    for (size_t i = 0; i < count; i++) {
        if (i > 0) {
            put(r, ",", 1);
        }
        put_json_string(r, values[i]);
    }
    put(r, "]", 1);
}

bool td_reply_end(td_reply_t *r)
{
    put(r, "}", 1);
    if (!r->overflow) {
        return true;
    }
    td_reply_error(r->buf, r->cap, r->cmd, "reply too long");
    return false;
}

bool td_reply_error(char *buf, size_t cap, const char *cmd, const char *message)
{
    td_reply_t r;
    td_reply_begin(&r, buf, cap, cmd, false);
    td_reply_str(&r, "error", message);
    put(&r, "}", 1);
    if (r.overflow && cap > 0) {
        buf[0] = '\0';
    }
    return !r.overflow;
}
