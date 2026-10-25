#include "td_rx_sync.h"

size_t td_rx_sync(td_rx_sync_t *s, char *buf, size_t n, size_t space, size_t *dropped)
{
    (void)s;
    (void)buf;
    if (space < n) {
        *dropped += n;
        return 0;
    }
    return n;
}
