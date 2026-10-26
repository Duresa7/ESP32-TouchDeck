#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Byte limits, without the terminator. A longer string is cut on a UTF-8 boundary. */
#define UI_WIDGET_ID_MAX 15
#define UI_WIDGET_VALUE_MAX 24
#define UI_WIDGET_CAPTION_MAX 16
#define UI_DASH_TEXT_MAX 16
#define UI_DASH_ROWS_MAX 8
#define UI_WIDGET_TILES_MAX 64
#define UI_DASH_PAGES_MAX 8

/* What the app last sent for one widget tile. `bar` is a fraction from 0 to 1, or -1 for no bar. */
typedef struct {
    char value[UI_WIDGET_VALUE_MAX + 1];
    char caption[UI_WIDGET_CAPTION_MAX + 1];
    float bar;
    bool stale;
    uint32_t updated_ms;
} ui_widget_state_t;

typedef struct {
    char label[UI_DASH_TEXT_MAX + 1];
    char value[UI_DASH_TEXT_MAX + 1];
    float bar;
} ui_dash_row_t;

/* The rows of one dashboard page. */
typedef struct {
    uint8_t count;
    ui_dash_row_t rows[UI_DASH_ROWS_MAX];
    uint32_t updated_ms;
} ui_dash_state_t;

/*
 * The state of every widget tile and dashboard page of the layout on screen, keyed by tile or page id. Only ids that
 * the layout registered take a set, so a set for an unknown id is refused. `dirty` marks an entry the screen has not
 * drawn yet. The table is plain data with no lock, so its owner guards it.
 */
typedef struct {
    struct {
        char id[UI_WIDGET_ID_MAX + 1];
        ui_widget_state_t state;
        bool dirty;
    } widgets[UI_WIDGET_TILES_MAX];
    uint8_t widget_count;
    struct {
        char id[UI_WIDGET_ID_MAX + 1];
        ui_dash_state_t state;
        bool dirty;
    } dashboards[UI_DASH_PAGES_MAX];
    uint8_t dashboard_count;
} ui_widget_table_t;

typedef enum {
    UI_WIDGET_OK = 0,
    UI_WIDGET_UNKNOWN,
    UI_WIDGET_TOO_MANY,
    UI_WIDGET_FULL,
} ui_widget_result_t;

/* The refusal as the link and the console word it: "", "unknown tile", "unknown page", "too many rows", "table full".
 */
const char *ui_widget_result_text(ui_widget_result_t result, bool dashboard);

/* Empties the table. */
void ui_widget_table_clear(ui_widget_table_t *table);

/* Registers a widget tile or a dashboard page with an empty state: no value, no caption and no bar. Registering an id
 * twice keeps one entry. UI_WIDGET_FULL when the table has no room, and UI_WIDGET_UNKNOWN for an empty or NULL id. */
ui_widget_result_t ui_widget_table_add_tile(ui_widget_table_t *table, const char *id);
ui_widget_result_t ui_widget_table_add_page(ui_widget_table_t *table, const char *id);

/* A bar as the table keeps it: -1 for a negative or NaN fraction, 1 for anything above 1. */
float ui_widget_clamp_bar(float bar);

/* Copies src into dst of `cap` bytes, cut on a UTF-8 boundary. NULL copies as the empty string. */
void ui_widget_copy_text(char *dst, size_t cap, const char *src);

/* Replaces a widget tile's state and marks it dirty. NULL strings are empty. */
ui_widget_result_t ui_widget_set(ui_widget_table_t *table, const char *tile, const char *value, const char *caption,
                                 float bar, bool stale, uint32_t now_ms);

/* Replaces a dashboard page's rows and marks it dirty. More than UI_DASH_ROWS_MAX rows are refused whole. */
ui_widget_result_t ui_dashboard_set(ui_widget_table_t *table, const char *page, const ui_dash_row_t *rows,
                                    size_t count, uint32_t now_ms);

/* The entry for an id, or NULL. */
const ui_widget_state_t *ui_widget_get(const ui_widget_table_t *table, const char *tile);
const ui_dash_state_t *ui_dashboard_get(const ui_widget_table_t *table, const char *page);

/* The index of an id in widgets[] or dashboards[], or -1. */
int ui_widget_index(const ui_widget_table_t *table, const char *tile);
int ui_dashboard_index(const ui_widget_table_t *table, const char *page);

#ifdef __cplusplus
}
#endif
