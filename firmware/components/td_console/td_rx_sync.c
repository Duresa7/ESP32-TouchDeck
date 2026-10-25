#include "td_rx_sync.h"
#include <string.h>

static bool is_eol(char c)
{
    return c == '\r' || c == '\n';
}

static size_t text_sync(td_rx_sync_t *s, char *buf, size_t n, size_t space, size_t *dropped)
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

size_t td_rx_sync(td_rx_sync_t *s, char *buf, size_t n, size_t space, size_t *dropped)
{
    size_t text_len = 0;
    for (size_t i = 0; i < n; i++) {
        uint8_t c = (uint8_t)buf[i];
        if (c == 0xc0) {
            if (s->mode == TD_RX_TEXT) {
                s->mode = TD_RX_FRAME;
                s->frame_len = 0;
            } else {
                if (s->on_frame && (s->frame_len || s->mode != TD_RX_FRAME)) {
                    bool valid = s->mode == TD_RX_FRAME;
                    s->on_frame(valid ? s->frame : NULL, valid ? s->frame_len : 0, s->context);
                }
                s->mode = TD_RX_TEXT;
            }
        } else if (s->mode == TD_RX_TEXT) {
            buf[text_len++] = (char)c;
        } else if (s->mode != TD_RX_FRAME_DISCARD) {
            if (s->mode == TD_RX_ESCAPE) {
                if (c != 0xdc && c != 0xdd) {
                    s->mode = TD_RX_FRAME_DISCARD;
                    continue;
                }
                c = c == 0xdc ? 0xc0 : 0xdb;
                s->mode = TD_RX_FRAME;
            } else if (c == 0xdb) {
                s->mode = TD_RX_ESCAPE;
                continue;
            }
            if (s->frame_len == sizeof s->frame) {
                s->mode = TD_RX_FRAME_DISCARD;
            } else {
                s->frame[s->frame_len++] = c;
            }
        }
    }
    return text_sync(s, buf, text_len, space, dropped);
}

td_line_status_t td_line_feed(td_line_t *s, int c, bool dropped,
                              char *line, size_t cap, char echo[3], size_t *echo_len)
{
    *echo_len = 0;
    if (c == TD_RX_ABANDON || (c < 0 && s->len == 0 && dropped)) {
        memcpy(echo, "\r\n", 2);
        *echo_len = 2;
        *s = (td_line_t){0};
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
        td_line_status_t status = s->too_long ? TD_LINE_TOO_LONG : TD_LINE_OK;
        *s = (td_line_t){0};
        return status;
    }
    if (c == '\b' || c == 0x7f) {
        if (s->len > 0 && !s->too_long) {
            s->len--;
            memcpy(echo, "\b \b", 3);
            *echo_len = 3;
        }
    } else if (s->len + 1 < cap) {
        line[s->len++] = (char)c;
        echo[0] = (char)c;
        *echo_len = 1;
    } else {
        s->too_long = true;
    }
    return TD_LINE_MORE;
}
