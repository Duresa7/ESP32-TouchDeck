#include "ui_page_report.h"

bool ui_page_report_due(ui_page_report_t *report, uint32_t session, uint32_t generation, uint8_t page)
{
    if (report->sent && report->session == session && report->generation == generation && report->page == page) {
        return false;
    }
    *report = (ui_page_report_t){ .session = session, .generation = generation, .page = page, .sent = true };
    return true;
}
