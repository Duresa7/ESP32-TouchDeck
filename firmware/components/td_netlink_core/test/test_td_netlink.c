#include "td_netlink.h"
#include "td_netlink_nvs.h"
#include "td_crc16.h"
#include "unity.h"
#include "cJSON.h"
#include "esp_heap_caps.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "nvs_flash.h"
#include "nvs.h"
#include <stdio.h>
#include <string.h>

extern const char vectors_start[] asm("_binary_net_v1_vectors_json_start");

typedef struct {
    uint8_t script[1024];
    size_t script_len;
    size_t script_at;
    uint32_t now;
    bool save_fails;
    int saves;
    td_netlink_apps_t saved;
} fixture_t;

static fixture_t s_fx;
static td_netlink_deck_t s_deck;
static td_netlink_conn_t s_conn;
static td_netlink_out_t s_out;
static uint8_t s_buf[TD_NETLINK_WIRE_MAX];
static uint8_t s_body[TD_NETLINK_WIRE_MAX];
static cJSON *s_root;

static unsigned hex_digit(char c)
{
    if (c >= '0' && c <= '9') {
        return (unsigned)(c - '0');
    }
    TEST_ASSERT_TRUE(c >= 'a' && c <= 'f');
    return (unsigned)(c - 'a' + 10);
}

static size_t unhex(const char *hex, uint8_t *out, size_t cap)
{
    TEST_ASSERT_NOT_NULL(hex);
    size_t len = strlen(hex);
    TEST_ASSERT_EQUAL_UINT(0, len % 2);
    TEST_ASSERT_LESS_OR_EQUAL_UINT(cap, len / 2);
    for (size_t i = 0; i < len / 2; i++) {
        out[i] = (uint8_t)((hex_digit(hex[2 * i]) << 4) | hex_digit(hex[2 * i + 1]));
    }
    return len / 2;
}

static const cJSON *node(const char *path)
{
    if (!s_root) {
        s_root = cJSON_Parse(vectors_start);
    }
    TEST_ASSERT_NOT_NULL(s_root);
    const cJSON *at = s_root;
    char name[48];
    while (*path) {
        size_t n = strcspn(path, ".");
        TEST_ASSERT_LESS_THAN_UINT(sizeof name, n);
        memcpy(name, path, n);
        name[n] = 0;
        at = cJSON_GetObjectItemCaseSensitive(at, name);
        TEST_ASSERT_NOT_NULL_MESSAGE(at, name);
        path += n + (path[n] == '.');
    }
    return at;
}

static const char *text(const char *path)
{
    const char *value = cJSON_GetStringValue(node(path));
    TEST_ASSERT_NOT_NULL(value);
    return value;
}

static size_t vec(const char *path, uint8_t *out, size_t cap)
{
    return unhex(text(path), out, cap);
}

static const char *record(int index, const char *field)
{
    const cJSON *item = cJSON_GetArrayItem(node("records"), index);
    TEST_ASSERT_NOT_NULL(item);
    const char *value = cJSON_GetStringValue(cJSON_GetObjectItemCaseSensitive(item, field));
    TEST_ASSERT_NOT_NULL(value);
    return value;
}

static void fx_rng(void *ctx, uint8_t *out, size_t len)
{
    fixture_t *fx = ctx;
    TEST_ASSERT_LESS_OR_EQUAL_UINT_MESSAGE(fx->script_len - fx->script_at, len, "RNG script exhausted");
    memcpy(out, fx->script + fx->script_at, len);
    fx->script_at += len;
}

static bool fx_save(void *ctx, const td_netlink_apps_t *apps)
{
    fixture_t *fx = ctx;
    if (fx->save_fails) {
        return false;
    }
    fx->saves++;
    fx->saved = *apps;
    return true;
}

static uint32_t fx_clock(void *ctx)
{
    return ((fixture_t *)ctx)->now;
}

static void script_bytes(const uint8_t *data, size_t len)
{
    TEST_ASSERT_LESS_OR_EQUAL_UINT(sizeof s_fx.script - s_fx.script_len, len);
    memcpy(s_fx.script + s_fx.script_len, data, len);
    s_fx.script_len += len;
}

static void script(const char *path)
{
    uint8_t value[64];
    script_bytes(value, vec(path, value, sizeof value));
}

static void script_u32(uint32_t value)
{
    const uint8_t raw[4] = {(uint8_t)(value >> 24), (uint8_t)(value >> 16), (uint8_t)(value >> 8), (uint8_t)value};
    script_bytes(raw, 4);
}

static void vector_app(td_netlink_apps_t *apps)
{
    memset(apps, 0, sizeof *apps);
    apps->count = 1;
    vec("inputs.appId", apps->apps[0].app_id, TD_NETLINK_APP_ID_LEN);
    vec("inputs.ltk", apps->apps[0].ltk, TD_NETLINK_KEY_LEN);
    const char *name = text("inputs.appName");
    apps->apps[0].name_len = (uint8_t)strlen(name);
    memcpy(apps->apps[0].name, name, strlen(name));
}

static void setup_deck(const td_netlink_apps_t *stored)
{
    memset(&s_fx, 0, sizeof s_fx);
    s_fx.now = 1000;
    uint8_t deck_id[TD_NETLINK_DECK_ID_LEN];
    vec("inputs.deckId", deck_id, sizeof deck_id);
    td_netlink_deck_init(&s_deck, deck_id, stored, fx_rng, fx_save, fx_clock, &s_fx);
    td_netlink_conn_init(&s_conn, &s_deck);
}

static td_netlink_action_t feed_bytes(const uint8_t *wire, size_t len)
{
    TEST_ASSERT_GREATER_OR_EQUAL_UINT(2, len);
    TEST_ASSERT_EQUAL_UINT(len - 2, (size_t)wire[0] << 8 | wire[1]);
    return td_netlink_conn_item(&s_conn, &s_deck, wire + 2, len - 2, &s_out);
}

static td_netlink_action_t feed(const char *path)
{
    return feed_bytes(s_buf, vec(path, s_buf, sizeof s_buf));
}

static void expect_item(const char *path)
{
    size_t len = vec(path, s_buf, sizeof s_buf);
    TEST_ASSERT_EQUAL_UINT(len, s_out.len);
    TEST_ASSERT_EQUAL_HEX8_ARRAY(s_buf, s_out.item, len);
}

static void expect_error(td_netlink_reason_t reason)
{
    const uint8_t item[] = {0x00, 0x03, TD_NETLINK_ERROR, TD_NETLINK_VERSION, (uint8_t)reason};
    TEST_ASSERT_EQUAL_UINT(sizeof item, s_out.len);
    TEST_ASSERT_EQUAL_HEX8_ARRAY(item, s_out.item, sizeof item);
}

static void script_vector_attempt(void)
{
    script("inputs.salt");
    script("inputs.b");
    script("inputs.deckNonce");
}

static void vector_session(void)
{
    td_netlink_apps_t apps;
    vector_app(&apps);
    setup_deck(&apps);
    script("inputs.sessionDeckNonce");
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND, feed("session.wire.open"));
    expect_item("session.wire.challenge");
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND, feed("session.wire.proof"));
    expect_item("session.wire.ready");
    TEST_ASSERT_EQUAL(TD_NETLINK_CONN_READY, s_conn.state);
}

static bool open_record(const char *hex)
{
    size_t len = unhex(hex, s_buf, sizeof s_buf);
    TEST_ASSERT_EQUAL_UINT(len - 2, (size_t)s_buf[0] << 8 | s_buf[1]);
    return td_netlink_conn_record(&s_conn, &s_deck, s_buf + 2, len - 2, s_body);
}

static size_t build_request(uint8_t *wire, const uint8_t *app_id, const uint8_t *name, size_t name_len)
{
    size_t body = 2 + TD_NETLINK_APP_ID_LEN + 1 + name_len;
    wire[0] = (uint8_t)(body >> 8);
    wire[1] = (uint8_t)body;
    wire[2] = TD_NETLINK_PAIR_REQUEST;
    wire[3] = TD_NETLINK_VERSION;
    memcpy(wire + 4, app_id, TD_NETLINK_APP_ID_LEN);
    wire[4 + TD_NETLINK_APP_ID_LEN] = (uint8_t)name_len;
    memcpy(wire + 5 + TD_NETLINK_APP_ID_LEN, name, name_len);
    return 2 + body;
}

TEST_CASE("SRP server reproduces the vector B and K_srp, including the leading-zero case", "[td_netlink]")
{
    static uint8_t salt[16], b[32], A[384], B[384], key[64], want[384];
    const char *identity = text("pake.identity");
    td_netlink_srp_in_t in = {
        .identity = (const uint8_t *)identity, .identity_len = strlen(identity),
        .code = text("inputs.code"), .salt = salt, .b = b, .A = A,
    };
    vec("inputs.salt", salt, sizeof salt);
    vec("inputs.b", b, sizeof b);
    TEST_ASSERT_EQUAL_UINT(384, vec("pake.A", A, sizeof A));
    TEST_ASSERT_EQUAL(TD_NETLINK_SRP_OK, td_netlink_srp_server(&in, B, key));
    TEST_ASSERT_EQUAL_UINT(384, vec("pake.B", want, sizeof want));
    TEST_ASSERT_EQUAL_HEX8_ARRAY(want, B, 384);
    vec("pake.K_srp", want, 64);
    TEST_ASSERT_EQUAL_HEX8_ARRAY(want, key, 64);

    vec("leading_zero_case.salt", salt, sizeof salt);
    TEST_ASSERT_EQUAL_UINT(384, vec("leading_zero_case.A", A, sizeof A));
    TEST_ASSERT_EQUAL_HEX8(0, A[0]);
    TEST_ASSERT_EQUAL(TD_NETLINK_SRP_OK, td_netlink_srp_server(&in, B, key));
    TEST_ASSERT_EQUAL_UINT(384, vec("leading_zero_case.B", want, sizeof want));
    TEST_ASSERT_EQUAL_HEX8_ARRAY(want, B, 384);
    vec("leading_zero_case.K_srp", want, 64);
    TEST_ASSERT_EQUAL_HEX8_ARRAY(want, key, 64);
}

TEST_CASE("SRP server refuses A of zero and A of N or more", "[td_netlink]")
{
    static uint8_t salt[16], b[32], A[384], B[384], key[64];
    const char *identity = text("pake.identity");
    td_netlink_srp_in_t in = {
        .identity = (const uint8_t *)identity, .identity_len = strlen(identity),
        .code = "123456", .salt = salt, .b = b, .A = A,
    };
    vec("inputs.b", b, sizeof b);
    memset(A, 0, sizeof A);
    TEST_ASSERT_EQUAL(TD_NETLINK_SRP_BAD_PUBLIC, td_netlink_srp_server(&in, B, key));
    vec("group.N", A, sizeof A);
    TEST_ASSERT_EQUAL(TD_NETLINK_SRP_BAD_PUBLIC, td_netlink_srp_server(&in, B, key));
    memset(A, 0xff, sizeof A);
    TEST_ASSERT_EQUAL(TD_NETLINK_SRP_BAD_PUBLIC, td_netlink_srp_server(&in, B, key));
}

TEST_CASE("pairing HKDF labels reproduce the vector keys and proofs", "[td_netlink]")
{
    uint8_t t[32], k[64], prk[32], out[32], want[32];
    vec("pairing.transcript", t, sizeof t);
    vec("pake.K_srp", k, sizeof k);
    TEST_ASSERT_TRUE(td_netlink_hmac(t, sizeof t, (const td_netlink_part_t[]) {{k, sizeof k}}, 1, prk));
    vec("pairing.prk", want, sizeof want);
    TEST_ASSERT_EQUAL_HEX8_ARRAY(want, prk, 32);
    const char *labels[][2] = {
        {"pair/client", "pairing.client"}, {"pair/deck", "pairing.deck"},
        {"pair/wrap", "pairing.wrap"}, {"pair/commit", "pairing.commit"},
    };
    for (size_t i = 0; i < 4; i++) {
        TEST_ASSERT_TRUE(td_netlink_derive(prk, labels[i][0], out));
        vec(labels[i][1], want, sizeof want);
        TEST_ASSERT_EQUAL_HEX8_ARRAY_MESSAGE(want, out, 32, labels[i][0]);
    }
}

TEST_CASE("pairing with the vector inputs sends the vector offer, challenge, accept, and stored", "[td_netlink]")
{
    setup_deck(NULL);
    script_u32(123456);
    script("inputs.windowId");
    script_vector_attempt();
    script("inputs.ltk");
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND, feed("pairing.wire.request"));
    expect_item("pairing.wire.offer");
    TEST_ASSERT_TRUE(s_deck.window.open);
    TEST_ASSERT_EQUAL_MEMORY("123456", s_deck.window.code, 6);
    s_fx.now += 30000;

    size_t internal_before = heap_caps_get_free_size(MALLOC_CAP_INTERNAL);
    TEST_ASSERT_EQUAL(ESP_OK, heap_caps_monitor_local_minimum_free_size_start());
    int64_t started = esp_timer_get_time();
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND, feed("pairing.wire.start"));
    int64_t took = esp_timer_get_time() - started;
    size_t internal_min = heap_caps_get_minimum_free_size(MALLOC_CAP_INTERNAL);
    TEST_ASSERT_EQUAL(ESP_OK, heap_caps_monitor_local_minimum_free_size_stop());
    printf("td_netlink: pair.start SRP-3072 step took %lld us, internal heap peak %u bytes\n",
           (long long)took, (unsigned)(internal_before - internal_min));
    expect_item("pairing.wire.challenge");

    TEST_ASSERT_EQUAL(TD_NETLINK_SEND, feed("pairing.wire.proof"));
    expect_item("pairing.wire.accept");
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND_CLOSE, feed("pairing.wire.commit"));
    expect_item("pairing.wire.stored");

    TEST_ASSERT_FALSE(s_deck.window.open);
    TEST_ASSERT_EQUAL(TD_NETLINK_CONN_CLOSED, s_conn.state);
    TEST_ASSERT_EQUAL_UINT(s_fx.script_len, s_fx.script_at);
    td_netlink_apps_t want;
    vector_app(&want);
    TEST_ASSERT_EQUAL(1, s_fx.saves);
    TEST_ASSERT_EQUAL_MEMORY(&want, &s_fx.saved, sizeof want);
    TEST_ASSERT_EQUAL_MEMORY(&want, &s_deck.apps, sizeof want);
}

TEST_CASE("a failed proof returns to code entry and the next attempt pairs", "[td_netlink]")
{
    setup_deck(NULL);
    script_u32(123456);
    script("inputs.windowId");
    script_vector_attempt();
    script_vector_attempt();
    script("inputs.ltk");
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND, feed("pairing.wire.request"));
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND, feed("pairing.wire.start"));
    const uint8_t zeros[36] = {0x00, 0x22, TD_NETLINK_PAIR_PROOF, TD_NETLINK_VERSION};
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND, feed_bytes(zeros, sizeof zeros));
    expect_error(TD_NETLINK_AUTH);
    TEST_ASSERT_EQUAL(TD_NETLINK_CONN_PAIR_CODE, s_conn.state);
    TEST_ASSERT_TRUE(s_deck.window.open);
    TEST_ASSERT_EQUAL(1, s_deck.window.failures);

    TEST_ASSERT_EQUAL(TD_NETLINK_SEND, feed("pairing.wire.start"));
    expect_item("pairing.wire.challenge");
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND, feed("pairing.wire.proof"));
    expect_item("pairing.wire.accept");
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND_CLOSE, feed("pairing.wire.commit"));
    expect_item("pairing.wire.stored");
    TEST_ASSERT_EQUAL(1, s_deck.apps.count);
}

TEST_CASE("three wrong codes end the window and store nothing", "[td_netlink]")
{
    setup_deck(NULL);
    script_u32(654321);
    script("inputs.windowId");
    for (int i = 0; i < 3; i++) {
        script_vector_attempt();
    }
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND, feed("pairing.wire.request"));
    expect_item("pairing.wire.offer");
    for (int i = 1; i <= 3; i++) {
        TEST_ASSERT_EQUAL(TD_NETLINK_SEND, feed("pairing.wire.start"));
        TEST_ASSERT_EQUAL(i < 3 ? TD_NETLINK_SEND : TD_NETLINK_SEND_CLOSE, feed("pairing.wire.proof"));
        expect_error(TD_NETLINK_AUTH);
    }
    TEST_ASSERT_FALSE(s_deck.window.open);
    TEST_ASSERT_EQUAL(TD_NETLINK_CONN_CLOSED, s_conn.state);
    TEST_ASSERT_EQUAL(0, s_fx.saves);
    TEST_ASSERT_EQUAL(0, s_deck.apps.count);
}

TEST_CASE("a bad commit proof or a failed store ends pairing without an entry", "[td_netlink]")
{
    setup_deck(NULL);
    script_u32(123456);
    script("inputs.windowId");
    script_vector_attempt();
    script("inputs.ltk");
    feed("pairing.wire.request");
    feed("pairing.wire.start");
    feed("pairing.wire.proof");
    size_t len = vec("pairing.wire.commit", s_buf, sizeof s_buf);
    s_buf[len - 1] ^= 1;
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND_CLOSE, feed_bytes(s_buf, len));
    expect_error(TD_NETLINK_AUTH);
    TEST_ASSERT_FALSE(s_deck.window.open);
    TEST_ASSERT_EQUAL(0, s_fx.saves);

    setup_deck(NULL);
    s_fx.save_fails = true;
    script_u32(123456);
    script("inputs.windowId");
    script_vector_attempt();
    script("inputs.ltk");
    feed("pairing.wire.request");
    feed("pairing.wire.start");
    feed("pairing.wire.proof");
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND_CLOSE, feed("pairing.wire.commit"));
    expect_error(TD_NETLINK_STORAGE);
    TEST_ASSERT_EQUAL(0, s_deck.apps.count);
}

TEST_CASE("cancel, window expiry, and the attempt deadline each end pairing", "[td_netlink]")
{
    setup_deck(NULL);
    script_u32(123456);
    script("inputs.windowId");
    feed("pairing.wire.request");
    TEST_ASSERT_EQUAL(TD_NETLINK_NOTHING, td_netlink_conn_tick(&s_conn, &s_deck, &s_out));
    td_netlink_window_cancel(&s_deck);
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND_CLOSE, td_netlink_conn_tick(&s_conn, &s_deck, &s_out));
    expect_error(TD_NETLINK_CANCELLED);

    setup_deck(NULL);
    script_u32(123456);
    script("inputs.windowId");
    feed("pairing.wire.request");
    s_fx.now += TD_NETLINK_WINDOW_MS - 1;
    TEST_ASSERT_EQUAL(TD_NETLINK_NOTHING, td_netlink_conn_tick(&s_conn, &s_deck, &s_out));
    s_fx.now += 1;
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND_CLOSE, td_netlink_conn_tick(&s_conn, &s_deck, &s_out));
    expect_error(TD_NETLINK_EXPIRED);
    TEST_ASSERT_FALSE(s_deck.window.open);

    setup_deck(NULL);
    script_u32(123456);
    script("inputs.windowId");
    script_vector_attempt();
    feed("pairing.wire.request");
    feed("pairing.wire.start");
    s_fx.now += TD_NETLINK_HANDSHAKE_MS;
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND_CLOSE, feed("pairing.wire.proof"));
    expect_error(TD_NETLINK_EXPIRED);
    TEST_ASSERT_FALSE(s_deck.window.open);
}

TEST_CASE("net pair opens an unbound window that a request binds with the time left", "[td_netlink]")
{
    setup_deck(NULL);
    script_u32(0xffffffffu);
    script_u32(123456);
    script("inputs.windowId");
    TEST_ASSERT_EQUAL(TD_NETLINK_REASON_NONE, td_netlink_window_open(&s_deck));
    TEST_ASSERT_EQUAL_MEMORY("123456", s_deck.window.code, 6);
    TEST_ASSERT_FALSE(s_deck.window.bound);
    TEST_ASSERT_EQUAL(TD_NETLINK_BUSY, td_netlink_window_open(&s_deck));
    s_fx.now += 30000;
    TEST_ASSERT_TRUE(td_netlink_deck_tick(&s_deck));
    td_netlink_conn_init(&s_conn, &s_deck);
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND, feed("pairing.wire.request"));
    size_t len = vec("pairing.wire.offer", s_buf, sizeof s_buf);
    const uint8_t left[4] = {0x00, 0x01, 0x5f, 0x90};
    memcpy(s_buf + len - 4, left, 4);
    TEST_ASSERT_EQUAL_UINT(len, s_out.len);
    TEST_ASSERT_EQUAL_HEX8_ARRAY(s_buf, s_out.item, len);
    TEST_ASSERT_TRUE(s_deck.window.bound);

    td_netlink_conn_close(&s_conn, &s_deck);
    TEST_ASSERT_FALSE(s_deck.window.open);
    TEST_ASSERT_EQUAL_UINT(s_fx.script_len, s_fx.script_at);

    setup_deck(NULL);
    script_u32(1);
    script("inputs.windowId");
    TEST_ASSERT_EQUAL(TD_NETLINK_REASON_NONE, td_netlink_window_open(&s_deck));
    s_fx.now += TD_NETLINK_WINDOW_MS;
    TEST_ASSERT_FALSE(td_netlink_deck_tick(&s_deck));
}

TEST_CASE("a full table or a known app id is refused before any code is drawn", "[td_netlink]")
{
    td_netlink_apps_t apps;
    memset(&apps, 0, sizeof apps);
    apps.count = TD_NETLINK_APPS_MAX;
    for (int i = 0; i < TD_NETLINK_APPS_MAX; i++) {
        apps.apps[i].app_id[0] = (uint8_t)(0xa0 + i);
        apps.apps[i].name_len = 1;
        apps.apps[i].name[0] = 'x';
    }
    setup_deck(&apps);
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND_CLOSE, feed("pairing.wire.request"));
    expect_error(TD_NETLINK_FULL);
    TEST_ASSERT_EQUAL(TD_NETLINK_FULL, td_netlink_window_open(&s_deck));
    TEST_ASSERT_FALSE(s_deck.window.open);

    vector_app(&apps);
    setup_deck(&apps);
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND_CLOSE, feed("pairing.wire.request"));
    expect_error(TD_NETLINK_FULL);
    TEST_ASSERT_FALSE(s_deck.window.open);
    TEST_ASSERT_EQUAL_UINT(0, s_fx.script_at);
}

TEST_CASE("malformed handshake items close with args or version", "[td_netlink]")
{
    static uint8_t wire[600];
    uint8_t app_id[TD_NETLINK_APP_ID_LEN];
    vec("inputs.appId", app_id, sizeof app_id);
    const struct {
        const char *name;
        size_t len;
    } bad_names[] = {
        {"", 0}, {"bell\x07", 5}, {"nul\0x", 5}, {"\xc0\x80", 2}, {"\xed\xa0\x80", 3},
        {"\xc2\x85", 2}, {"cut\xe2\x82", 5},
    };
    for (size_t i = 0; i < sizeof bad_names / sizeof bad_names[0]; i++) {
        setup_deck(NULL);
        size_t len = build_request(wire, app_id, (const uint8_t *)bad_names[i].name, bad_names[i].len);
        TEST_ASSERT_EQUAL(TD_NETLINK_SEND_CLOSE, feed_bytes(wire, len));
        expect_error(TD_NETLINK_ARGS);
    }
    setup_deck(NULL);
    script_u32(1);
    script("inputs.windowId");
    size_t len = build_request(wire, app_id, (const uint8_t *)"caf\xc3\xa9 \xf0\x9f\x8e\x9b", 10);
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND, feed_bytes(wire, len));

    setup_deck(NULL);
    len = vec("pairing.wire.request", wire, sizeof wire);
    wire[1]++;
    wire[len++] = 0;
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND_CLOSE, feed_bytes(wire, len));
    expect_error(TD_NETLINK_ARGS);

    setup_deck(NULL);
    len = vec("pairing.wire.request", wire, sizeof wire);
    wire[3] = 2;
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND_CLOSE, feed_bytes(wire, len));
    expect_error(TD_NETLINK_BAD_VERSION);

    setup_deck(NULL);
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND_CLOSE, feed("pairing.wire.proof"));
    expect_error(TD_NETLINK_ARGS);

    setup_deck(NULL);
    script_u32(1);
    script("inputs.windowId");
    script_vector_attempt();
    feed("pairing.wire.request");
    len = vec("pairing.wire.start", wire, sizeof wire);
    memset(wire + 4, 0, 384);
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND_CLOSE, feed_bytes(wire, len));
    expect_error(TD_NETLINK_ARGS);
    TEST_ASSERT_FALSE(s_deck.window.open);
}

TEST_CASE("the first item must arrive within 10 s of accept", "[td_netlink]")
{
    setup_deck(NULL);
    s_fx.now += TD_NETLINK_HANDSHAKE_MS - 1;
    TEST_ASSERT_EQUAL(TD_NETLINK_NOTHING, td_netlink_conn_tick(&s_conn, &s_deck, &s_out));
    s_fx.now += 1;
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND_CLOSE, feed("pairing.wire.request"));
    expect_error(TD_NETLINK_EXPIRED);
    TEST_ASSERT_FALSE(s_deck.window.open);
}

TEST_CASE("session handshake and records reproduce the vectors both ways", "[td_netlink]")
{
    vector_session();
    for (int i = 0; i < 2; i++) {
        TEST_ASSERT_TRUE(open_record(record(i, "wire")));
        uint8_t want[64];
        size_t len = unhex(record(i, "plaintext"), want, sizeof want);
        TEST_ASSERT_EQUAL_HEX8_ARRAY(want, s_body, len);
    }
    for (int i = 2; i < 4; i++) {
        uint8_t plain[128], want[160], wire[160];
        size_t len = unhex(record(i, "plaintext"), plain, sizeof plain);
        size_t wire_len = unhex(record(i, "wire"), want, sizeof want);
        TEST_ASSERT_EQUAL_UINT(len + 18, wire_len);
        TEST_ASSERT_TRUE(td_netlink_conn_send(&s_conn, plain, len, wire));
        TEST_ASSERT_EQUAL_HEX8_ARRAY(want, wire, wire_len);
    }
}

TEST_CASE("tampered, replayed, and reordered records close the session", "[td_netlink]")
{
    vector_session();
    TEST_ASSERT_FALSE(open_record(text("tampered_record.wire")));
    TEST_ASSERT_EQUAL(TD_NETLINK_CONN_CLOSED, s_conn.state);

    vector_session();
    TEST_ASSERT_TRUE(open_record(record(0, "wire")));
    TEST_ASSERT_FALSE(open_record(record(0, "wire")));
    TEST_ASSERT_EQUAL(TD_NETLINK_CONN_CLOSED, s_conn.state);
    TEST_ASSERT_FALSE(open_record(record(1, "wire")));

    vector_session();
    TEST_ASSERT_FALSE(open_record(record(1, "wire")));
    TEST_ASSERT_EQUAL(TD_NETLINK_CONN_CLOSED, s_conn.state);

    vector_session();
    size_t len = unhex(record(0, "wire"), s_buf, sizeof s_buf);
    TEST_ASSERT_FALSE(td_netlink_conn_record(&s_conn, &s_deck, s_buf + 2, len - 3, s_body));
}

TEST_CASE("session open fails with auth for a wrong deck, an unknown app, or a bad proof", "[td_netlink]")
{
    td_netlink_apps_t apps;
    vector_app(&apps);
    setup_deck(&apps);
    size_t len = vec("session.wire.open", s_buf, sizeof s_buf);
    s_buf[4] ^= 1;
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND_CLOSE, feed_bytes(s_buf, len));
    expect_error(TD_NETLINK_AUTH);

    setup_deck(NULL);
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND_CLOSE, feed("session.wire.open"));
    expect_error(TD_NETLINK_AUTH);

    setup_deck(&apps);
    script("inputs.sessionDeckNonce");
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND, feed("session.wire.open"));
    len = vec("session.wire.proof", s_buf, sizeof s_buf);
    s_buf[len - 1] ^= 1;
    TEST_ASSERT_EQUAL(TD_NETLINK_SEND_CLOSE, feed_bytes(s_buf, len));
    expect_error(TD_NETLINK_AUTH);
    TEST_ASSERT_EQUAL(1, s_deck.apps.count);
}

TEST_CASE("a session closes 5 s after ready without hello, then 5 s after the last record", "[td_netlink]")
{
    vector_session();
    s_fx.now += TD_NETLINK_ONLINE_MS - 1;
    TEST_ASSERT_EQUAL(TD_NETLINK_NOTHING, td_netlink_conn_tick(&s_conn, &s_deck, &s_out));
    s_fx.now += 1;
    TEST_ASSERT_EQUAL(TD_NETLINK_CLOSE, td_netlink_conn_tick(&s_conn, &s_deck, &s_out));
    TEST_ASSERT_EQUAL(TD_NETLINK_CONN_CLOSED, s_conn.state);

    vector_session();
    s_fx.now += 3000;
    TEST_ASSERT_TRUE(open_record(record(0, "wire")));
    td_netlink_conn_hello(&s_conn);
    s_fx.now += TD_NETLINK_ONLINE_MS - 1;
    TEST_ASSERT_EQUAL(TD_NETLINK_NOTHING, td_netlink_conn_tick(&s_conn, &s_deck, &s_out));
    s_fx.now += 1;
    TEST_ASSERT_EQUAL(TD_NETLINK_CLOSE, td_netlink_conn_tick(&s_conn, &s_deck, &s_out));
}

static size_t body_with_payload(uint8_t *body, size_t payload_len)
{
    body[0] = 1;
    body[1] = 1;
    body[2] = 9;
    body[3] = 0;
    body[4] = (uint8_t)payload_len;
    body[5] = (uint8_t)(payload_len >> 8);
    memset(body + 6, '7', payload_len);
    uint16_t crc = td_crc16(body, 6 + payload_len);
    body[6 + payload_len] = (uint8_t)crc;
    body[7 + payload_len] = (uint8_t)(crc >> 8);
    return payload_len + 8;
}

TEST_CASE("records carry a maximum frame, refuse a bad body, and stop at counter 2^32", "[td_netlink]")
{
    static uint8_t body[TD_NETLINK_PLAIN_MAX], wire[TD_NETLINK_WIRE_MAX], back[TD_NETLINK_SEALED_MAX];
    uint8_t key[32];
    vec("session.c2d", key, sizeof key);
    td_netlink_dir_t tx, rx;
    td_netlink_dir_init(&tx, TD_NETLINK_C2D, key);
    td_netlink_dir_init(&rx, TD_NETLINK_C2D, key);
    size_t len = body_with_payload(body, 1024);
    TEST_ASSERT_EQUAL_UINT(TD_NETLINK_PLAIN_MAX, len);
    TEST_ASSERT_TRUE(td_netlink_record_seal(&tx, body, len, wire));
    TEST_ASSERT_EQUAL_UINT(TD_NETLINK_SEALED_MAX, (size_t)wire[0] << 8 | wire[1]);
    TEST_ASSERT_TRUE(td_netlink_record_open(&rx, wire + 2, TD_NETLINK_SEALED_MAX, back));
    TEST_ASSERT_EQUAL_HEX8_ARRAY(body, back, len);

    body[len - 1] ^= 1;
    TEST_ASSERT_FALSE(td_netlink_record_seal(&tx, body, len, wire));
    TEST_ASSERT_TRUE(tx.closed);

    td_netlink_dir_init(&tx, TD_NETLINK_C2D, key);
    td_netlink_dir_init(&rx, TD_NETLINK_C2D, key);
    tx.counter = TD_NETLINK_COUNTER_LIMIT - 1;
    rx.counter = TD_NETLINK_COUNTER_LIMIT - 1;
    len = body_with_payload(body, 4);
    TEST_ASSERT_TRUE(td_netlink_record_seal(&tx, body, len, wire));
    TEST_ASSERT_TRUE(td_netlink_record_open(&rx, wire + 2, len + 16, back));
    TEST_ASSERT_FALSE(td_netlink_record_seal(&tx, body, len, wire));
    TEST_ASSERT_FALSE(td_netlink_record_open(&rx, wire + 2, len + 16, back));

    td_netlink_dir_init(&tx, TD_NETLINK_C2D, key);
    td_netlink_dir_init(&rx, TD_NETLINK_D2C, key);
    TEST_ASSERT_TRUE(td_netlink_record_seal(&tx, body, len, wire));
    TEST_ASSERT_FALSE_MESSAGE(td_netlink_record_open(&rx, wire + 2, len + 16, back), "reflected record");
}

TEST_CASE("forget cancels a window and erases every pairing", "[td_netlink]")
{
    td_netlink_apps_t apps;
    vector_app(&apps);
    setup_deck(&apps);
    script_u32(1);
    script("inputs.windowId");
    TEST_ASSERT_EQUAL(TD_NETLINK_REASON_NONE, td_netlink_window_open(&s_deck));
    TEST_ASSERT_TRUE(td_netlink_forget(&s_deck));
    TEST_ASSERT_FALSE(s_deck.window.open);
    TEST_ASSERT_EQUAL(0, s_deck.apps.count);
    TEST_ASSERT_EQUAL(0, s_fx.saved.count);

    setup_deck(&apps);
    s_fx.save_fails = true;
    TEST_ASSERT_FALSE(td_netlink_forget(&s_deck));
    TEST_ASSERT_EQUAL(0, s_deck.apps.count);
}

TEST_CASE("the NVS store round-trips the app table and erases it", "[td_netlink]")
{
    static const char ns[] = "td_netlink_test";
    esp_err_t err = nvs_flash_init();
    TEST_ASSERT_EQUAL_MESSAGE(ESP_OK, err, "NVS init failed; the test never erases the partition");
    td_netlink_apps_t apps, back;
    vector_app(&apps);
    apps.count = 2;
    apps.apps[1] = apps.apps[0];
    apps.apps[1].app_id[0] ^= 0xff;
    apps.apps[1].name_len = 1;
    TEST_ASSERT_TRUE(td_netlink_nvs_save(ns, &apps));
    TEST_ASSERT_TRUE(td_netlink_nvs_load(ns, &back));
    TEST_ASSERT_EQUAL(2, back.count);
    TEST_ASSERT_EQUAL_MEMORY(&apps.apps[0], &back.apps[0], sizeof apps.apps[0]);
    TEST_ASSERT_EQUAL_MEMORY(apps.apps[1].ltk, back.apps[1].ltk, TD_NETLINK_KEY_LEN);
    TEST_ASSERT_EQUAL(1, back.apps[1].name_len);

    const td_netlink_apps_t empty = {0};
    TEST_ASSERT_TRUE(td_netlink_nvs_save(ns, &empty));
    TEST_ASSERT_TRUE(td_netlink_nvs_load(ns, &back));
    TEST_ASSERT_EQUAL(0, back.count);

    nvs_handle_t handle;
    TEST_ASSERT_EQUAL(ESP_OK, nvs_open(ns, NVS_READWRITE, &handle));
    TEST_ASSERT_EQUAL(ESP_OK, nvs_erase_all(handle));
    TEST_ASSERT_EQUAL(ESP_OK, nvs_commit(handle));
    nvs_close(handle);
}

typedef struct {
    TaskHandle_t waiter;
    td_netlink_srp_result_t result;
    int64_t took_us;
    UBaseType_t stack_left;
} srp_run_t;

static void srp_task(void *arg)
{
    srp_run_t *run = arg;
    static uint8_t salt[16], b[32], A[384], B[384], key[64];
    for (int i = 0; i < 16; i++) {
        salt[i] = (uint8_t)(0x30 + i);
    }
    for (int i = 0; i < 32; i++) {
        b[i] = (uint8_t)(0x70 + i);
    }
    memset(A, 0, sizeof A);
    A[383] = 5;
    static const char identity[] = "touchdeck:020000123456:101112131415161718191a1b1c1d1e1f";
    td_netlink_srp_in_t in = {
        .identity = (const uint8_t *)identity, .identity_len = sizeof identity - 1,
        .code = "123456", .salt = salt, .b = b, .A = A,
    };
    int64_t started = esp_timer_get_time();
    run->result = td_netlink_srp_server(&in, B, key);
    run->took_us = esp_timer_get_time() - started;
    run->stack_left = uxTaskGetStackHighWaterMark(NULL);
    xTaskNotifyGive(run->waiter);
    vTaskDelete(NULL);
}

TEST_CASE("one SRP-3072 server step fits a 6 KB task stack", "[td_netlink]")
{
    const uint32_t stack = 6144;
    srp_run_t run = {.waiter = xTaskGetCurrentTaskHandle()};
    TEST_ASSERT_EQUAL(pdPASS, xTaskCreate(srp_task, "srp", stack, &run, 1, NULL));
    TEST_ASSERT_EQUAL(1, ulTaskNotifyTake(pdTRUE, pdMS_TO_TICKS(20000)));
    TEST_ASSERT_EQUAL(TD_NETLINK_SRP_OK, run.result);
    printf("td_netlink: SRP-3072 server step %lld us on a priority-1 task, stack used %u of %u bytes\n",
           (long long)run.took_us, (unsigned)(stack - run.stack_left), (unsigned)stack);
}
