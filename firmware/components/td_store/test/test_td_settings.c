#include <stdio.h>
#include <string.h>
#include "unity.h"
#include "td_settings.h"

static td_settings_t parse(const char *json, bool *is_object)
{
    td_settings_t s;
    const bool ok = td_settings_parse(json, strlen(json), &s);
    if (is_object != NULL) {
        *is_object = ok;
    }
    return s;
}

static void assert_settings(uint32_t idle, td_orientation_t orientation, td_settings_t s)
{
    TEST_ASSERT_EQUAL_UINT32(idle, s.idle_timeout_s);
    TEST_ASSERT_EQUAL_INT(orientation, s.orientation);
}

TEST_CASE("defaults are 600 seconds and landscape", "[td_settings]")
{
    td_settings_t s = { .idle_timeout_s = 7, .orientation = TD_ORIENTATION_PORTRAIT };
    td_settings_defaults(&s);
    assert_settings(600, TD_ORIENTATION_LANDSCAPE, s);

    bool is_object = false;
    assert_settings(600, TD_ORIENTATION_LANDSCAPE, parse("{}", &is_object));
    TEST_ASSERT_TRUE(is_object);
}

TEST_CASE("each field alone leaves the other at its default", "[td_settings]")
{
    assert_settings(120, TD_ORIENTATION_LANDSCAPE, parse("{\"idle_timeout_s\":120}", NULL));
    assert_settings(600, TD_ORIENTATION_PORTRAIT, parse("{\"orientation\":\"portrait\"}", NULL));
    assert_settings(600, TD_ORIENTATION_LANDSCAPE, parse("{\"orientation\":\"landscape\"}", NULL));
    assert_settings(1, TD_ORIENTATION_LANDSCAPE, parse("{\"idle_timeout_s\":1}", NULL));
    assert_settings(86400, TD_ORIENTATION_LANDSCAPE, parse("{\"idle_timeout_s\":86400}", NULL));
}

TEST_CASE("both fields are read together", "[td_settings]")
{
    assert_settings(45, TD_ORIENTATION_PORTRAIT, parse("{\"idle_timeout_s\":45,\"orientation\":\"portrait\"}", NULL));
    assert_settings(45, TD_ORIENTATION_PORTRAIT, parse("{ \"orientation\" : \"portrait\" , \"idle_timeout_s\" : 45 }", NULL));
}

TEST_CASE("an out-of-range timeout takes the default", "[td_settings]")
{
    assert_settings(600, TD_ORIENTATION_LANDSCAPE, parse("{\"idle_timeout_s\":0}", NULL));
    assert_settings(600, TD_ORIENTATION_LANDSCAPE, parse("{\"idle_timeout_s\":86401}", NULL));
    assert_settings(600, TD_ORIENTATION_LANDSCAPE, parse("{\"idle_timeout_s\":-5}", NULL));
    assert_settings(600, TD_ORIENTATION_LANDSCAPE, parse("{\"idle_timeout_s\":4294967296}", NULL));
    assert_settings(600, TD_ORIENTATION_LANDSCAPE, parse("{\"idle_timeout_s\":1e30}", NULL));
    assert_settings(600, TD_ORIENTATION_LANDSCAPE, parse("{\"idle_timeout_s\":12.5}", NULL));
    assert_settings(600, TD_ORIENTATION_LANDSCAPE, parse("{\"idle_timeout_s\":\"120\"}", NULL));
    assert_settings(600, TD_ORIENTATION_LANDSCAPE, parse("{\"idle_timeout_s\":null}", NULL));
    assert_settings(600, TD_ORIENTATION_LANDSCAPE, parse("{\"idle_timeout_s\":true}", NULL));
}

TEST_CASE("an out-of-range timeout does not disturb a good orientation", "[td_settings]")
{
    assert_settings(600, TD_ORIENTATION_PORTRAIT, parse("{\"idle_timeout_s\":0,\"orientation\":\"portrait\"}", NULL));
}

TEST_CASE("an unknown orientation takes landscape", "[td_settings]")
{
    assert_settings(90, TD_ORIENTATION_LANDSCAPE, parse("{\"idle_timeout_s\":90,\"orientation\":\"sideways\"}", NULL));
    assert_settings(600, TD_ORIENTATION_LANDSCAPE, parse("{\"orientation\":\"Portrait\"}", NULL));
    assert_settings(600, TD_ORIENTATION_LANDSCAPE, parse("{\"orientation\":\"\"}", NULL));
    assert_settings(600, TD_ORIENTATION_LANDSCAPE, parse("{\"orientation\":1}", NULL));
    assert_settings(600, TD_ORIENTATION_LANDSCAPE, parse("{\"orientation\":null}", NULL));
}

TEST_CASE("an unknown field is ignored", "[td_settings]")
{
    assert_settings(30, TD_ORIENTATION_PORTRAIT,
                    parse("{\"brightness\":9,\"idle_timeout_s\":30,\"nested\":{\"a\":[1,2]},\"orientation\":\"portrait\"}", NULL));
    assert_settings(600, TD_ORIENTATION_LANDSCAPE, parse("{\"Idle_Timeout_S\":30}", NULL));
}

TEST_CASE("a file that is not a JSON object gives the defaults", "[td_settings]")
{
    const char *bad[] = { "", "   ", "not json", "[1,2,3]", "42", "\"portrait\"", "null", "{\"idle_timeout_s\":" };
    for (size_t i = 0; i < sizeof bad / sizeof bad[0]; i++) {
        bool is_object = true;
        assert_settings(600, TD_ORIENTATION_LANDSCAPE, parse(bad[i], &is_object));
        TEST_ASSERT_FALSE_MESSAGE(is_object, bad[i]);
    }
    td_settings_t s;
    TEST_ASSERT_FALSE(td_settings_parse(NULL, 0, &s));
    assert_settings(600, TD_ORIENTATION_LANDSCAPE, s);
}

TEST_CASE("parse reads only the bytes it was given", "[td_settings]")
{
    const char text[] = "{\"idle_timeout_s\":120}garbage";
    td_settings_t s;
    TEST_ASSERT_TRUE(td_settings_parse(text, sizeof("{\"idle_timeout_s\":120}") - 1, &s));
    assert_settings(120, TD_ORIENTATION_LANDSCAPE, s);
}

TEST_CASE("the formatter writes the file the parser reads back", "[td_settings]")
{
    const td_settings_t in[] = {
        { 600, TD_ORIENTATION_LANDSCAPE },
        { 1, TD_ORIENTATION_PORTRAIT },
        { 86400, TD_ORIENTATION_LANDSCAPE },
        { 123, TD_ORIENTATION_PORTRAIT },
    };
    for (size_t i = 0; i < sizeof in / sizeof in[0]; i++) {
        char buf[TD_SETTINGS_JSON_MAX];
        const size_t len = td_settings_format(&in[i], buf, sizeof buf);
        TEST_ASSERT_GREATER_THAN(0, len);
        TEST_ASSERT_EQUAL_UINT(strlen(buf), len);
        td_settings_t out;
        TEST_ASSERT_TRUE(td_settings_parse(buf, len, &out));
        assert_settings(in[i].idle_timeout_s, in[i].orientation, out);
    }

    char buf[TD_SETTINGS_JSON_MAX];
    const td_settings_t s = { 120, TD_ORIENTATION_PORTRAIT };
    td_settings_format(&s, buf, sizeof buf);
    TEST_ASSERT_EQUAL_STRING("{\"idle_timeout_s\":120,\"orientation\":\"portrait\"}", buf);
}

TEST_CASE("the formatter refuses what it could not read back", "[td_settings]")
{
    char buf[TD_SETTINGS_JSON_MAX];
    const td_settings_t zero = { 0, TD_ORIENTATION_LANDSCAPE };
    const td_settings_t big = { 86401, TD_ORIENTATION_LANDSCAPE };
    const td_settings_t bad_orientation = { 60, (td_orientation_t)7 };
    const td_settings_t good = { 60, TD_ORIENTATION_LANDSCAPE };
    TEST_ASSERT_EQUAL_UINT(0, td_settings_format(&zero, buf, sizeof buf));
    TEST_ASSERT_EQUAL_UINT(0, td_settings_format(&big, buf, sizeof buf));
    TEST_ASSERT_EQUAL_UINT(0, td_settings_format(&bad_orientation, buf, sizeof buf));
    TEST_ASSERT_EQUAL_UINT(0, td_settings_format(&good, buf, 10));
    TEST_ASSERT_GREATER_THAN(0, td_settings_format(&good, buf, TD_SETTINGS_JSON_MAX));
}

TEST_CASE("the longest settings file fits TD_SETTINGS_JSON_MAX", "[td_settings]")
{
    char buf[TD_SETTINGS_JSON_MAX];
    const td_settings_t s = { 86400, TD_ORIENTATION_LANDSCAPE };
    const size_t len = td_settings_format(&s, buf, sizeof buf);
    TEST_ASSERT_GREATER_THAN(0, len);
    TEST_ASSERT_LESS_THAN(TD_SETTINGS_JSON_MAX, len);
}

TEST_CASE("orientation names parse exactly", "[td_settings]")
{
    td_orientation_t o = TD_ORIENTATION_PORTRAIT;
    TEST_ASSERT_TRUE(td_settings_orientation_parse("landscape", &o));
    TEST_ASSERT_EQUAL_INT(TD_ORIENTATION_LANDSCAPE, o);
    TEST_ASSERT_TRUE(td_settings_orientation_parse("portrait", &o));
    TEST_ASSERT_EQUAL_INT(TD_ORIENTATION_PORTRAIT, o);
    TEST_ASSERT_FALSE(td_settings_orientation_parse("Landscape", &o));
    TEST_ASSERT_FALSE(td_settings_orientation_parse("", &o));
    TEST_ASSERT_FALSE(td_settings_orientation_parse(NULL, &o));
    TEST_ASSERT_EQUAL_INT(TD_ORIENTATION_PORTRAIT, o);
    TEST_ASSERT_EQUAL_STRING("landscape", td_settings_orientation_name(TD_ORIENTATION_LANDSCAPE));
    TEST_ASSERT_EQUAL_STRING("portrait", td_settings_orientation_name(TD_ORIENTATION_PORTRAIT));
}

TEST_CASE("store paths stay under /storage with plain names", "[td_store]")
{
    const char *good[] = {
        "/storage/settings.json", "/storage/layouts/landscape.json", "/storage/icons/0123456789abcdef.bin",
        "/storage/recovery", "/storage/a", "/storage/a-b_c.d.e", "/storage/test.bin",
    };
    for (size_t i = 0; i < sizeof good / sizeof good[0]; i++) {
        TEST_ASSERT_TRUE_MESSAGE(td_store_path_valid(good[i], false), good[i]);
    }

    const char *bad[] = {
        "", "storage/a", "/storage/", "/storage//a", "/storage/a/", "/storage/./a", "/storage/../a", "/storage/a/..",
        "/storage/a/../b", "/storagex/a", "/storag/a", "/tmp/a", "/storage/a b", "/storage/a\\b", "/storage/a:b",
        "/storage/a\n", "/storage/\xc3\xa9", "/storage/a*", "/storage/..", "/storage/.", "//storage/a",
    };
    for (size_t i = 0; i < sizeof bad / sizeof bad[0]; i++) {
        TEST_ASSERT_FALSE_MESSAGE(td_store_path_valid(bad[i], false), bad[i]);
    }
    TEST_ASSERT_FALSE(td_store_path_valid(NULL, true));
}

TEST_CASE("the bare mount point is a path only when the caller allows it", "[td_store]")
{
    TEST_ASSERT_TRUE(td_store_path_valid("/storage", true));
    TEST_ASSERT_FALSE(td_store_path_valid("/storage", false));
    TEST_ASSERT_FALSE(td_store_path_valid("/storagea", true));
    TEST_ASSERT_FALSE(td_store_path_valid("/storage/", true));
}

TEST_CASE("store path and segment lengths are bounded", "[td_store]")
{
    char path[160];
    /* "/storage/" is 9 characters, so 86 more make 95, the longest path, in three segments. */
    snprintf(path, sizeof path, "/storage/%.28s/%.28s/%.28s", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
             "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "cccccccccccccccccccccccccccccccccccc");
    TEST_ASSERT_EQUAL_UINT(9 + 28 * 3 + 2, strlen(path));
    TEST_ASSERT_TRUE(td_store_path_valid(path, false));

    snprintf(path, sizeof path, "/storage/%.32s", "dddddddddddddddddddddddddddddddddddddddd");
    TEST_ASSERT_TRUE(td_store_path_valid(path, false));
    snprintf(path, sizeof path, "/storage/%.33s", "dddddddddddddddddddddddddddddddddddddddd");
    TEST_ASSERT_FALSE(td_store_path_valid(path, false));

    snprintf(path, sizeof path, "/storage/%.32s/%.32s/%.32s", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
             "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "cccccccccccccccccccccccccccccccccccc");
    TEST_ASSERT_TRUE(strlen(path) > 95);
    TEST_ASSERT_FALSE(td_store_path_valid(path, false));
}

TEST_CASE("base64 encodes the RFC 4648 vectors", "[td_store]")
{
    static const struct {
        const char *plain;
        const char *encoded;
    } vectors[] = {
        { "f", "Zg==" }, { "fo", "Zm8=" }, { "foo", "Zm9v" }, { "foob", "Zm9vYg==" }, { "fooba", "Zm9vYmE=" },
        { "foobar", "Zm9vYmFy" },
    };
    for (size_t i = 0; i < sizeof vectors / sizeof vectors[0]; i++) {
        char out[16];
        const size_t n = td_store_b64_encode((const uint8_t *)vectors[i].plain, strlen(vectors[i].plain), out, sizeof out);
        TEST_ASSERT_EQUAL_UINT(strlen(vectors[i].encoded), n);
        TEST_ASSERT_EQUAL_STRING(vectors[i].encoded, out);

        uint8_t back[16];
        size_t back_len = 0;
        TEST_ASSERT_TRUE(td_store_b64_decode(vectors[i].encoded, strlen(vectors[i].encoded), back, sizeof back, &back_len));
        TEST_ASSERT_EQUAL_UINT(strlen(vectors[i].plain), back_len);
        TEST_ASSERT_EQUAL_MEMORY(vectors[i].plain, back, back_len);
    }
}

TEST_CASE("base64 encode needs room for the NUL and some input", "[td_store]")
{
    char out[8];
    TEST_ASSERT_EQUAL_UINT(0, td_store_b64_encode((const uint8_t *)"foo", 3, out, 4));
    TEST_ASSERT_EQUAL_UINT(4, td_store_b64_encode((const uint8_t *)"foo", 3, out, 5));
    TEST_ASSERT_EQUAL_UINT(0, td_store_b64_encode((const uint8_t *)"foo", 0, out, sizeof out));
    TEST_ASSERT_EQUAL_UINT(0, td_store_b64_encode((const uint8_t *)"foobar!", 7, out, sizeof out));
}

TEST_CASE("a 180-byte line round trips through base64 without padding", "[td_store]")
{
    uint8_t raw[180];
    for (size_t i = 0; i < sizeof raw; i++) {
        raw[i] = (uint8_t)(i * 31 + 7);
    }
    char text[4 * (180 / 3) + 1];
    const size_t n = td_store_b64_encode(raw, sizeof raw, text, sizeof text);
    TEST_ASSERT_EQUAL_UINT(240, n);
    TEST_ASSERT_NULL(strchr(text, '='));

    uint8_t back[180];
    size_t back_len = 0;
    TEST_ASSERT_TRUE(td_store_b64_decode(text, n, back, sizeof back, &back_len));
    TEST_ASSERT_EQUAL_UINT(180, back_len);
    TEST_ASSERT_EQUAL_MEMORY(raw, back, sizeof raw);

    TEST_ASSERT_FALSE(td_store_b64_decode(text, n, back, 179, &back_len));
}

TEST_CASE("every byte value round trips through base64", "[td_store]")
{
    uint8_t raw[256];
    for (size_t i = 0; i < sizeof raw; i++) {
        raw[i] = (uint8_t)i;
    }
    for (size_t len = 1; len <= 7; len++) {
        char text[16];
        uint8_t back[8];
        size_t back_len = 0;
        const size_t n = td_store_b64_encode(raw + 250, len, text, sizeof text);
        TEST_ASSERT_GREATER_THAN(0, n);
        TEST_ASSERT_TRUE(td_store_b64_decode(text, n, back, sizeof back, &back_len));
        TEST_ASSERT_EQUAL_UINT(len, back_len);
        TEST_ASSERT_EQUAL_MEMORY(raw + 250, back, len);
    }
    char text[4 * 86 + 1];
    uint8_t back[256];
    size_t back_len = 0;
    const size_t n = td_store_b64_encode(raw, sizeof raw, text, sizeof text);
    TEST_ASSERT_EQUAL_UINT(344, n);
    TEST_ASSERT_TRUE(td_store_b64_decode(text, n, back, sizeof back, &back_len));
    TEST_ASSERT_EQUAL_UINT(256, back_len);
    TEST_ASSERT_EQUAL_MEMORY(raw, back, sizeof raw);
}

TEST_CASE("base64 decode refuses what is not canonical", "[td_store]")
{
    const char *bad[] = {
        "", "Zg=", "Zg", "Z", "Zm9", "Zm9v=", "Zg=A", "=g==", "Z===", "Zh==", "Zm9=", "Zm9vYmF", "Zm9v\n", "Zm 9", "Zm9-",
        "Zm9_", "Zg==Zg==", "Zm9vZg==Zg==", "Zm9v Zg==",
    };
    for (size_t i = 0; i < sizeof bad / sizeof bad[0]; i++) {
        uint8_t out[16];
        size_t out_len = 99;
        TEST_ASSERT_FALSE_MESSAGE(td_store_b64_decode(bad[i], strlen(bad[i]), out, sizeof out, &out_len), bad[i]);
        TEST_ASSERT_EQUAL_UINT(99, out_len);
    }
    uint8_t out[16];
    size_t out_len = 0;
    TEST_ASSERT_FALSE(td_store_b64_decode(NULL, 4, out, sizeof out, &out_len));
}
