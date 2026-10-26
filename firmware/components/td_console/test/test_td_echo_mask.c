#include <string.h>
#include "td_rx_sync.h"
#include "unity.h"

static void check_echo(const char *input, const char *expected, const char *stored)
{
    const td_echo_mask_t mask = { .prefix = "wifi set", .after_words = 3 };
    td_line_t state = { .masks = &mask, .mask_count = 1 };
    char line[256], output[512] = "";
    size_t used = 0;
    for (const char *p = input; *p; p++) {
        char echo[3];
        size_t n;
        td_line_feed(&state, (unsigned char)*p, false, line, sizeof line, echo, &n);
        memcpy(output + used, echo, n);
        used += n;
    }
    output[used] = '\0';
    if (state.len) line[state.len] = '\0';
    TEST_ASSERT_EQUAL_STRING(expected, output);
    TEST_ASSERT_EQUAL_STRING(stored, line);
}

TEST_CASE("secret command masks every password byte before dispatch", "[td_console]")
{
    check_echo("wifi set deck password\n", "wifi set deck ********\r\n", "wifi set deck password");
    check_echo("  \"wifi\" \"set\" \"my deck\" \"pass word\"\n",
               "  \"wifi\" \"set\" \"my deck\" ***********\r\n",
               "  \"wifi\" \"set\" \"my deck\" \"pass word\"");
    check_echo("wifi set my\\ deck a\\ b\\\"c\\\\d\n",
               "wifi set my\\ deck ************\r\n", "wifi set my\\ deck a\\ b\\\"c\\\\d");
    check_echo("wifi state\n", "wifi state\r\n", "wifi state");
    check_echo("other set deck password\n", "other set deck password\r\n", "other set deck password");
}

TEST_CASE("secret echo survives editing and resets on the next line", "[td_console]")
{
    check_echo("wifi set deck ab\bcd\nheap\n", "wifi set deck **\b \b**\r\nheap\r\n", "heap");
    check_echo("wifi set deck password", "wifi set deck ********", "wifi set deck password");
}
