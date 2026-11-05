#include "unity.h"
#include "ui_page_report.h"

TEST_CASE("the page on screen is reported once a session and on every change", "[ui_page_report]")
{
    ui_page_report_t report = { 0 };
    TEST_ASSERT_TRUE(ui_page_report_due(&report, 1, 7, 0));
    TEST_ASSERT_FALSE(ui_page_report_due(&report, 1, 7, 0));
    TEST_ASSERT_TRUE(ui_page_report_due(&report, 1, 7, 2));
    TEST_ASSERT_FALSE(ui_page_report_due(&report, 1, 7, 2));
    TEST_ASSERT_TRUE(ui_page_report_due(&report, 1, 7, 0));
    TEST_ASSERT_TRUE(ui_page_report_due(&report, 1, 8, 0));
    TEST_ASSERT_FALSE(ui_page_report_due(&report, 1, 8, 0));
    TEST_ASSERT_TRUE(ui_page_report_due(&report, 2, 8, 0));
    TEST_ASSERT_FALSE(ui_page_report_due(&report, 2, 8, 0));
}

TEST_CASE("the first report goes out even for session, layout and page zero", "[ui_page_report]")
{
    ui_page_report_t report = { 0 };
    TEST_ASSERT_TRUE(ui_page_report_due(&report, 0, 0, 0));
    TEST_ASSERT_FALSE(ui_page_report_due(&report, 0, 0, 0));
}
