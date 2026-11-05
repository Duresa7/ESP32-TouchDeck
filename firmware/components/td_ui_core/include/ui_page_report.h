#pragma once

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* What the link last reported as the Page on screen. Zeroed means nothing reported yet. */
typedef struct {
    uint32_t session;
    uint32_t generation;
    uint8_t page;
    bool sent;
} ui_page_report_t;

/* True when the link should send `v2.page` for `page`: once in each protocol 2 `session`, then whenever the Page or
 * the Layout `generation` changes. A true answer records the report as sent. */
bool ui_page_report_due(ui_page_report_t *report, uint32_t session, uint32_t generation, uint8_t page);

#ifdef __cplusplus
}
#endif
