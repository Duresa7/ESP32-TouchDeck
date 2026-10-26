#include "td_rx_sync.h"
#include <string.h>
#include "esp_console.h"

static bool line_secret(const char *line, size_t len, const td_echo_mask_t *mask)
{
    char copy[256];
    char *argv[32];
    if (!mask || !mask->prefix || len >= sizeof copy) return false;
    memcpy(copy, line, len);
    copy[len] = '\0';
    const size_t argc = esp_console_split_argv(copy, argv, sizeof argv / sizeof argv[0]);
    if (argc <= mask->after_words) return false;
    const char *prefix = mask->prefix;
    for (size_t i = 0; i < argc; i++) {
        const size_t n = strcspn(prefix, " ");
        if (strlen(argv[i]) != n || memcmp(argv[i], prefix, n) != 0) return false;
        prefix += n;
        if (!*prefix) return true;
        prefix++;
    }
    return false;
}

static void reset_line(td_line_t *s)
{
    *s = (td_line_t){ .masks = s->masks, .mask_count = s->mask_count };
}

static bool is_eol(char c)
{
    return c == '\r' || c == '\n';
}

size_t td_rx_sync(td_rx_sync_t *s, char *buf, size_t n, size_t space, size_t *dropped)
{
    size_t from = 0;
    if (s->discarding) {
        while (from < n && !is_eol(buf[from])) {
            from++;
        }
        if (from == n) {
            *dropped += n;
            return 0;
        }
        from++;
        *dropped += from;
        s->discarding = false;
    }

    size_t keep = n - from;
    if (keep == 0) {
        return 0;
    }

    if (space > keep) {
        for (size_t i = 0; i < keep; i++) {
            char c = buf[from + i];
            s->tail = is_eol(c) ? 0 : s->tail + 1;
            buf[i] = c == TD_RX_ABANDON ? '?' : c;
        }
        return keep;
    }

    *dropped += keep + s->tail;
    s->discarding = !is_eol(buf[n - 1]);
    bool abandon = s->tail > 0;
    s->tail = 0;
    if (!abandon) {
        return 0;
    }
    buf[0] = TD_RX_ABANDON;
    return 1;
}

#define FRAME_VERSION 0x01

static void end_frame(td_rx_split_t *s, td_rx_frame_fn frame, void *context)
{
    if (s->mode == TD_RX_FRAME) {
        frame(s->frame, s->frame_len, context);
    } else if (s->mode == TD_RX_ESCAPE || s->mode == TD_RX_FRAME_DISCARD) {
        frame(NULL, 0, context);
    }
}

static void store(td_rx_split_t *s, uint8_t c)
{
    if (s->frame_len == sizeof s->frame) {
        s->mode = TD_RX_FRAME_DISCARD;
    } else {
        s->frame[s->frame_len++] = c;
        s->mode = TD_RX_FRAME;
    }
}

uint32_t td_rx_split_expire(td_rx_split_t *s, uint32_t now_ms, td_rx_frame_fn frame, void *context)
{
    if (s->mode == TD_RX_TEXT) {
        return TD_RX_NO_DEADLINE;
    }
    uint32_t age = now_ms - s->opened_ms;
    if (age < TD_RX_FRAME_DEADLINE_MS) {
        return TD_RX_FRAME_DEADLINE_MS - age;
    }
    end_frame(s, frame, context);
    s->mode = TD_RX_TEXT;
    return TD_RX_NO_DEADLINE;
}

void td_rx_split(td_rx_split_t *s, uint32_t now_ms, char *buf, size_t n,
                 td_rx_text_fn text, td_rx_frame_fn frame, void *context)
{
    td_rx_split_expire(s, now_ms, frame, context);
    size_t text_len = 0;
    for (size_t i = 0; i < n; i++) {
        uint8_t c = (uint8_t)buf[i];
        if (c == 0xc0) {
            if (text_len) {
                text(buf, text_len, context);
                text_len = 0;
            }
            end_frame(s, frame, context);
            s->mode = TD_RX_OPEN;
            s->opened_ms = now_ms;
            s->frame_len = 0;
        } else if (s->mode == TD_RX_TEXT || (s->mode == TD_RX_OPEN && c != FRAME_VERSION)) {
            s->mode = TD_RX_TEXT;
            buf[text_len++] = (char)c;
        } else if (s->mode == TD_RX_ESCAPE) {
            if (c == 0xdc || c == 0xdd) {
                store(s, c == 0xdc ? 0xc0 : 0xdb);
            } else {
                s->mode = TD_RX_FRAME_DISCARD;
            }
        } else if (s->mode != TD_RX_FRAME_DISCARD) {
            if (c == 0xdb) {
                s->mode = TD_RX_ESCAPE;
            } else {
                store(s, c);
            }
        }
    }
    if (text_len) {
        text(buf, text_len, context);
    }
}

static bool has_control(const char *line, size_t len)
{
    for (size_t i = 0; i < len; i++) {
        if ((unsigned char)line[i] < 0x20 && line[i] != '\t') {
            return true;
        }
    }
    return false;
}

td_line_status_t td_line_feed(td_line_t *s, int c, bool dropped,
                              char *line, size_t cap, char echo[3], size_t *echo_len)
{
    *echo_len = 0;
    if (c == TD_RX_ABANDON || (c < 0 && s->len == 0 && dropped)) {
        memcpy(echo, "\r\n", 2);
        *echo_len = 2;
        reset_line(s);
        line[0] = '\0';
        return TD_LINE_RESTART;
    }
    if (c < 0) {
        return TD_LINE_MORE;
    }
    if (c == '\r' || c == '\n') {
        memcpy(echo, "\r\n", 2);
        *echo_len = 2;
        line[s->len] = '\0';
        td_line_status_t status = s->too_long ? TD_LINE_TOO_LONG
                                : has_control(line, s->len) ? TD_LINE_CONTROL : TD_LINE_OK;
        reset_line(s);
        return status;
    }
    if (c == '\b' || c == 0x7f) {
        if (s->len > 0 && !s->too_long) {
            s->len--;
            if (s->len < s->secret_from) s->secret_from = 0;
            memcpy(echo, "\b \b", 3);
            *echo_len = 3;
        }
    } else if (s->len + 1 < cap) {
        line[s->len++] = (char)c;
        for (size_t i = 0; i < s->mask_count; i++) {
            if (line_secret(line, s->len, &s->masks[i])) {
                if (!s->secret_from || s->len < s->secret_from) s->secret_from = s->len;
                break;
            }
        }
        echo[0] = s->secret_from && s->len >= s->secret_from ? '*' : (char)c;
        *echo_len = 1;
    } else {
        s->too_long = true;
    }
    return TD_LINE_MORE;
}
