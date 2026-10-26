#include "key_sink_fanout.h"

static void report(void *ctx, const key_report_t *value)
{
    key_sink_fanout_t *fanout = ctx;
    for (unsigned i = 0; i < KEY_SINK_FANOUT_MAX; i++) {
        if (fanout->sinks[i].report) {
            fanout->sinks[i].report(fanout->sinks[i].ctx, value);
        }
    }
}

key_sink_t key_sink_fanout_sink(key_sink_fanout_t *fanout)
{
    return (key_sink_t){ .report = report, .ctx = fanout };
}
