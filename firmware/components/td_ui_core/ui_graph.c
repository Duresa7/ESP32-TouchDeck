#include "ui_graph.h"
#include <math.h>

double ui_graph_ceiling(double m)
{
    double p = 1.0;
    while (p < m) {
        p *= 10.0;
    }
    while (p / 10.0 >= m) {
        p /= 10.0;
    }
    const double q = p / 10.0;
    if (m <= q * 2.0) {
        return q * 2.0;
    }
    if (m <= q * 5.0) {
        return q * 5.0;
    }
    return p;
}

ui_graph_range_t ui_graph_range(const double *samples, uint8_t count)
{
    double min = 0.0;
    double max = 0.0;
    for (uint8_t i = 0; i < count; i++) {
        min = samples[i] < min ? samples[i] : min;
        max = samples[i] > max ? samples[i] : max;
    }
    ui_graph_range_t range = {
        .lo = min < 0.0 ? -ui_graph_ceiling(-min) : 0.0,
        .hi = max > 0.0 ? ui_graph_ceiling(max) : 0.0,
    };
    if (range.hi == range.lo) {
        range.hi = range.lo + 1.0;
    }
    return range;
}

uint8_t ui_graph_points(const double *samples, uint8_t count, uint8_t cap, int16_t w, int16_t h, ui_graph_point_t *out)
{
    if (count == 0 || w < 2 || h < 2 || cap == 0) {
        return 0;
    }
    if (count > cap) {
        samples += count - cap;
        count = cap;
    }
    const ui_graph_range_t range = ui_graph_range(samples, count);
    for (uint8_t i = 0; i < count; i++) {
        const int32_t slot = cap - count + i;
        out[i].x = (int16_t)(cap == 1 ? w - 1 : slot * (w - 1) / (cap - 1));
        const double y = floor((samples[i] - range.lo) * (double)(h - 1) / (range.hi - range.lo) + 0.5);
        out[i].y = (int16_t)((h - 1) - (int32_t)y);
    }
    return count;
}
