#pragma once

#include <stdbool.h>
#include <string.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Whether a label that shows `shown` can skip a set to `text`: it shows the same characters from another buffer.
 * LVGL 9.6 copies the text and invalidates the label on every set, even an unchanged one. The same pointer is never
 * skipped: that is a label showing a static string, and the set is what makes LVGL take its own copy of it.
 */
static inline bool ui_text_unchanged(const char *shown, const char *text)
{
    return shown != NULL && shown != text && strcmp(shown, text) == 0;
}

/*
 * Whether a dots-mode label's text was cut to fit. LVGL 9.6 writes the three dots over the tail of its own copy of the
 * text, so a cut label never shows what was sent, and comparing it would skip or take a set for the wrong reason.
 */
static inline bool ui_text_cut_with_dots(const char *shown)
{
    const size_t len = shown != NULL ? strlen(shown) : 0;
    return len >= 3 && strcmp(shown + len - 3, "...") == 0;
}

#ifdef __cplusplus
}
#endif
