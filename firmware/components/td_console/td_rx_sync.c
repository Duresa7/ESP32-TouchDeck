#include "td_rx_sync.h"

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
