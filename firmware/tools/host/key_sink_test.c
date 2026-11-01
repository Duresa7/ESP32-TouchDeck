#include "key_sink_link.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

int main(void)
{
    key_sink_t sink = key_sink_link_sink();
    key_report_t kbd = key_report_kbd(255, 255);
    memset(kbd.kbd.keys, 255, sizeof kbd.kbd.keys);
    key_report_t con = key_report_con(65535);
    for (unsigned mode = 0; mode < 3; mode++) {
        for (size_t cap = 0; cap <= KEY_SINK_LINK_EVENT_MAX; cap++) {
            unsigned char buffer[512];
            memset(buffer, 0xa5, sizeof buffer);
            key_sink_link_discard();
            sink.report(sink.ctx, &con);
            if (mode != 1) {
                key_sink_link_discard();
                for (unsigned i = 0; i < (mode == 2 ? KEY_SINK_LINK_QUEUE + 1u : 1u); i++) {
                    sink.report(sink.ctx, &kbd);
                }
            }
            size_t n = key_sink_link_next((char *)buffer, cap);
            for (size_t i = cap; i < sizeof buffer; i++) {
                if (buffer[i] != 0xa5) {
                    fprintf(stderr, "write beyond capacity: mode=%u cap=%zu offset=%zu\n", mode, cap, i);
                    return 1;
                }
            }
            assert(n == 0 || (n < cap && buffer[n] == 0 && buffer[n - 1] == '}'));
            if (cap == KEY_SINK_LINK_EVENT_MAX) {
                assert(n > 0);
                assert(strstr((char *)buffer, mode == 2 ? "key.resync" : "key.report") != NULL);
                assert(strstr((char *)buffer, mode == 1 ? "65535" : "255,255,255,255,255,255") != NULL);
            }
        }
    }
    return 0;
}
