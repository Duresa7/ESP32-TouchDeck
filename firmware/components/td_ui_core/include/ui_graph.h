#pragma once

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* A vertex of a graph line, in pixels from the plot area's top-left corner. */
typedef struct {
    int16_t x;
    int16_t y;
} ui_graph_point_t;

/* The value range a graph spans: 0 to a round ceiling above the samples, or a round floor to 0 below them. */
typedef struct {
    double lo;
    double hi;
} ui_graph_range_t;

/* The smallest of 1, 2 or 5 times a power of ten that is at least m, for m above 0. */
double ui_graph_ceiling(double m);

/* The range of `count` samples. Both ends include 0, so a flat line sits at its true height. All zeros span 0 to 1. */
ui_graph_range_t ui_graph_range(const double *samples, uint8_t count);

/*
 * Places `count` samples, oldest first, in a plot area of w by h pixels that holds `cap` of them. The newest sample
 * sits on the right edge and each older one a cap-th of the width to its left, so a short history grows in from the
 * right. Sample i is at x = slot * (w - 1) / (cap - 1) in integer division, where slot = cap - count + i, and at
 * y = (h - 1) - floor((v - lo) * (h - 1) / (hi - lo) + 0.5). Returns how many points it wrote to out, which holds
 * `count`, or 0 for no samples or an area under 2 by 2. app/ui/src/lib/deck/graph.ts draws the preview with the
 * same arithmetic, and schema/live/graph-vectors.json holds the shared cases.
 */
uint8_t ui_graph_points(const double *samples, uint8_t count, uint8_t cap, int16_t w, int16_t h, ui_graph_point_t *out);

#ifdef __cplusplus
}
#endif
