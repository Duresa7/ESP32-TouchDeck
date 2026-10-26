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
    check_echo("wifi set deck a\b\bx y", "wifi set deck *\b \b\b \bx *", "wifi set deckx y");
}

TEST_CASE("secret masks apply only to registered prefixes and word counts", "[td_console]")
{
    const td_echo_mask_t masks[] = { { .prefix = "wifi set", .after_words = 3 }, { .prefix = "net pair", .after_words = 2 } };
    td_line_t state = { .masks = masks, .mask_count = 2 };
    char line[64], echo[3];
    size_t n;
    const char *input = "net pair 123456\nwifi set deck\nwifi sets deck pw\n";
    char output[160] = "";
    for (const char *p = input; *p; p++) {
        td_line_feed(&state, (unsigned char)*p, false, line, sizeof line, echo, &n);
        strncat(output, echo, n);
    }
    TEST_ASSERT_EQUAL_STRING("net pair ******\r\nwifi set deck\r\nwifi sets deck pw\r\n", output);
}

TEST_CASE("a line with no masks registered echoes every byte", "[td_console]")
{
    td_line_t state = { 0 };
    char line[64], echo[3];
    size_t n;
    for (const char *p = "wifi set deck password"; *p; p++) {
        td_line_feed(&state, (unsigned char)*p, false, line, sizeof line, echo, &n);
        TEST_ASSERT_EQUAL_UINT(1, n);
        TEST_ASSERT_EQUAL_CHAR(*p, echo[0]);
    }
}
