#include <stdio.h>
#include <string.h>
#include "unity.h"
#include "td_cmd_table.h"

static int run(int argc, char **argv)
{
    return 0;
}

static int run_with_context(void *context, int argc, char **argv)
{
    return 0;
}

#define CMD(name) { .command = (name), .help = "test", .func = run }

TEST_CASE("table lists commands in the order they were added", "[td_cmd_table]")
{
    td_cmd_table_t t = { 0 };
    const esp_console_cmd_t first[] = { CMD("help"), CMD("version") };
    const esp_console_cmd_t second[] = { CMD("screenshot"), CMD("perf"), CMD("backlight") };
    TEST_ASSERT_EQUAL(ESP_OK, td_cmd_table_add(&t, first, 2));
    TEST_ASSERT_EQUAL(ESP_OK, td_cmd_table_add(&t, second, 3));

    const char *names[TD_CMD_TABLE_MAX];
    TEST_ASSERT_EQUAL(5, td_cmd_table_names(&t, names));
    const char *want[] = { "help", "version", "screenshot", "perf", "backlight" };
    for (size_t i = 0; i < 5; i++) {
        TEST_ASSERT_EQUAL_STRING(want[i], names[i]);
    }
}

TEST_CASE("a sealed table refuses more commands and keeps its contents", "[td_cmd_table]")
{
    td_cmd_table_t t = { 0 };
    const esp_console_cmd_t early[] = { CMD("help") };
    const esp_console_cmd_t late[] = { CMD("screenshot") };
    TEST_ASSERT_EQUAL(ESP_OK, td_cmd_table_add(&t, early, 1));
    td_cmd_table_seal(&t);

    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_STATE, td_cmd_table_add(&t, late, 1));
    const char *names[TD_CMD_TABLE_MAX];
    TEST_ASSERT_EQUAL(1, td_cmd_table_names(&t, names));
    TEST_ASSERT_EQUAL_STRING("help", names[0]);
}

TEST_CASE("a name already in the table is refused, even inside one batch", "[td_cmd_table]")
{
    td_cmd_table_t t = { 0 };
    const esp_console_cmd_t first[] = { CMD("help"), CMD("version") };
    TEST_ASSERT_EQUAL(ESP_OK, td_cmd_table_add(&t, first, 2));

    const esp_console_cmd_t clash[] = { CMD("perf"), CMD("version") };
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_ARG, td_cmd_table_add(&t, clash, 2));
    const esp_console_cmd_t twice[] = { CMD("perf"), CMD("perf") };
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_ARG, td_cmd_table_add(&t, twice, 2));

    const char *names[TD_CMD_TABLE_MAX];
    TEST_ASSERT_EQUAL_MESSAGE(2, td_cmd_table_names(&t, names), "a refused batch must add nothing");
}

TEST_CASE("a command esp_console would refuse is refused up front", "[td_cmd_table]")
{
    td_cmd_table_t t = { 0 };
    const esp_console_cmd_t no_name = { .help = "test", .func = run };
    const esp_console_cmd_t empty_name = CMD("");
    const esp_console_cmd_t spaced_name = CMD("two words");
    const esp_console_cmd_t no_func = { .command = "nofunc" };
    const esp_console_cmd_t both_funcs = { .command = "both", .func = run, .func_w_context = run_with_context };
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_ARG, td_cmd_table_add(&t, &no_name, 1));
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_ARG, td_cmd_table_add(&t, &empty_name, 1));
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_ARG, td_cmd_table_add(&t, &spaced_name, 1));
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_ARG, td_cmd_table_add(&t, &no_func, 1));
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_ARG, td_cmd_table_add(&t, &both_funcs, 1));
    TEST_ASSERT_EQUAL(0, t.count);

    const esp_console_cmd_t context_only = { .command = "ctx", .func_w_context = run_with_context };
    TEST_ASSERT_EQUAL(ESP_OK, td_cmd_table_add(&t, &context_only, 1));
}

TEST_CASE("a full table refuses a batch that does not fit and adds none of it", "[td_cmd_table]")
{
    static td_cmd_table_t t;
    static char names_buf[TD_CMD_TABLE_MAX + 1][4];
    static esp_console_cmd_t cmds[TD_CMD_TABLE_MAX + 1];
    memset(&t, 0, sizeof t);
    for (size_t i = 0; i <= TD_CMD_TABLE_MAX; i++) {
        snprintf(names_buf[i], sizeof names_buf[i], "c%02u", (unsigned)i);
        cmds[i] = (esp_console_cmd_t)CMD(names_buf[i]);
    }

    TEST_ASSERT_EQUAL(ESP_ERR_NO_MEM, td_cmd_table_add(&t, cmds, TD_CMD_TABLE_MAX + 1));
    TEST_ASSERT_EQUAL(0, t.count);
    TEST_ASSERT_EQUAL(ESP_OK, td_cmd_table_add(&t, cmds, TD_CMD_TABLE_MAX));
    TEST_ASSERT_EQUAL(ESP_ERR_NO_MEM, td_cmd_table_add(&t, &cmds[TD_CMD_TABLE_MAX], 1));
    TEST_ASSERT_EQUAL(TD_CMD_TABLE_MAX, t.count);
}
