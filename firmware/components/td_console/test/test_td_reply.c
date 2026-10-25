#include <limits.h>
#include <string.h>
#include "unity.h"
#include "td_console.h"

#define GUARD 0x5a

TEST_CASE("plain reply carries cmd, ok and fields in order", "[td_reply]")
{
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "heap", true);
    td_reply_int(&r, "free_heap", 123456);
    td_reply_bool(&r, "psram", true);
    td_reply_str(&r, "chip", "esp32s3");
    TEST_ASSERT_TRUE(td_reply_end(&r));
    TEST_ASSERT_EQUAL_STRING("@td {\"cmd\":\"heap\",\"ok\":true,\"free_heap\":123456,\"psram\":true,\"chip\":\"esp32s3\"}", buf);
}

TEST_CASE("reply with no fields is still an object", "[td_reply]")
{
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "reboot", true);
    TEST_ASSERT_TRUE(td_reply_end(&r));
    TEST_ASSERT_EQUAL_STRING("@td {\"cmd\":\"reboot\",\"ok\":true}", buf);
}

TEST_CASE("reply escapes quotes, backslashes and control characters", "[td_reply]")
{
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "echo", true);
    td_reply_str(&r, "text", "say \"hi\" C:\\dir\nline\ttab\x01");
    TEST_ASSERT_TRUE(td_reply_end(&r));
    TEST_ASSERT_EQUAL_STRING("@td {\"cmd\":\"echo\",\"ok\":true,\"text\":\"say \\\"hi\\\" C:\\\\dir\\nline\\ttab\\u0001\"}", buf);
}

TEST_CASE("reply escapes the command name it echoes", "[td_reply]")
{
    char buf[TD_REPLY_MAX];
    TEST_ASSERT_TRUE(td_reply_error(buf, sizeof buf, "a\"b\\c", "unknown command"));
    TEST_ASSERT_EQUAL_STRING("@td {\"cmd\":\"a\\\"b\\\\c\",\"ok\":false,\"error\":\"unknown command\"}", buf);
}

TEST_CASE("error reply has ok false and an error string", "[td_reply]")
{
    char buf[TD_REPLY_MAX];
    TEST_ASSERT_TRUE(td_reply_error(buf, sizeof buf, "nosuchcmd", "unknown command"));
    TEST_ASSERT_EQUAL_STRING("@td {\"cmd\":\"nosuchcmd\",\"ok\":false,\"error\":\"unknown command\"}", buf);
}

TEST_CASE("string list renders as a json array", "[td_reply]")
{
    char buf[TD_REPLY_MAX];
    const char *names[] = {"help", "version"};
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "help", true);
    td_reply_strs(&r, "commands", names, 2);
    td_reply_strs(&r, "none", names, 0);
    TEST_ASSERT_TRUE(td_reply_end(&r));
    TEST_ASSERT_EQUAL_STRING("@td {\"cmd\":\"help\",\"ok\":true,\"commands\":[\"help\",\"version\"],\"none\":[]}", buf);
}

TEST_CASE("reply that does not fit becomes a valid error reply", "[td_reply]")
{
    char buf[64];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "heap", true);
    td_reply_str(&r, "text", "this value is far too long to fit in the sixty-four byte buffer");
    TEST_ASSERT_FALSE(td_reply_end(&r));
    TEST_ASSERT_EQUAL_STRING("@td {\"cmd\":\"heap\",\"ok\":false,\"error\":\"reply too long\"}", buf);
}

TEST_CASE("reply that exactly fills the buffer is kept", "[td_reply]")
{
    char storage[65];
    storage[64] = GUARD;
    td_reply_t r;
    td_reply_begin(&r, storage, 64, "x", true);
    td_reply_str(&r, "k", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    TEST_ASSERT_TRUE(td_reply_end(&r));
    TEST_ASSERT_EQUAL_STRING("@td {\"cmd\":\"x\",\"ok\":true,\"k\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}", storage);
    TEST_ASSERT_EQUAL_UINT(63, strlen(storage));
    TEST_ASSERT_EQUAL_UINT8(GUARD, storage[64]);
}

TEST_CASE("reply one byte past the buffer becomes the reply too long error", "[td_reply]")
{
    char storage[65];
    storage[64] = GUARD;
    td_reply_t r;
    td_reply_begin(&r, storage, 64, "x", true);
    td_reply_str(&r, "k", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    TEST_ASSERT_FALSE(td_reply_end(&r));
    TEST_ASSERT_EQUAL_STRING("@td {\"cmd\":\"x\",\"ok\":false,\"error\":\"reply too long\"}", storage);
    TEST_ASSERT_EQUAL_UINT8(GUARD, storage[64]);
}

TEST_CASE("reply escapes a carriage return", "[td_reply]")
{
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "echo", true);
    td_reply_str(&r, "text", "line\r\n");
    TEST_ASSERT_TRUE(td_reply_end(&r));
    TEST_ASSERT_EQUAL_STRING("@td {\"cmd\":\"echo\",\"ok\":true,\"text\":\"line\\r\\n\"}", buf);
}

TEST_CASE("reply renders negative integers", "[td_reply]")
{
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "x", true);
    td_reply_int(&r, "small", -42);
    td_reply_int(&r, "min", LLONG_MIN);
    TEST_ASSERT_TRUE(td_reply_end(&r));
    TEST_ASSERT_EQUAL_STRING("@td {\"cmd\":\"x\",\"ok\":true,\"small\":-42,\"min\":-9223372036854775808}", buf);
}

TEST_CASE("buffer too small for any reply is left empty", "[td_reply]")
{
    char buf[8] = "garbage";
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "heap", true);
    td_reply_int(&r, "free_heap", 1);
    TEST_ASSERT_FALSE(td_reply_end(&r));
    TEST_ASSERT_EQUAL_STRING("", buf);
}
