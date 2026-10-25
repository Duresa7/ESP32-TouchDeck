#include "td_frame.h"
#include "td_crc16.h"
#include "unity.h"
#include "cJSON.h"
#include <string.h>

extern const char golden_start[] asm("_binary_golden_frames_json_start");

static uint8_t s_wire[TD_FRAME_WIRE_MAX + 8];
static uint8_t s_encoded[TD_FRAME_WIRE_MAX];
static uint8_t s_body[TD_FRAME_BODY_MAX];
static uint8_t s_payload[TD_FRAME_PAYLOAD_MAX + 1];

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
    size_t len = strlen(hex);
    TEST_ASSERT_EQUAL_UINT(0, len % 2);
    TEST_ASSERT_LESS_OR_EQUAL_UINT(cap, len / 2);
    for (size_t i = 0; i < len / 2; i++) {
        out[i] = (uint8_t)((hex_digit(hex[2 * i]) << 4) | hex_digit(hex[2 * i + 1]));
    }
    return len / 2;
}

static int number(const cJSON *object, const char *name)
{
    const cJSON *item = cJSON_GetObjectItemCaseSensitive(object, name);
    TEST_ASSERT_TRUE(cJSON_IsNumber(item));
    return item->valueint;
}

TEST_CASE("CRC-16 CCITT FALSE matches the published check value", "[td_frame]")
{
    TEST_ASSERT_EQUAL_HEX16(0x29b1, td_crc16((const uint8_t *)"123456789", 9));
    TEST_ASSERT_EQUAL_HEX16(0xffff, td_crc16(NULL, 0));
}

TEST_CASE("every golden frame decodes and every valid fixture encodes exactly", "[td_frame]")
{
    cJSON *root = cJSON_Parse(golden_start);
    TEST_ASSERT_NOT_NULL(root);
    const cJSON *frames = cJSON_GetObjectItemCaseSensitive(root, "frames");
    TEST_ASSERT_GREATER_OR_EQUAL_INT(12, cJSON_GetArraySize(frames));
    const cJSON *entry;
    cJSON_ArrayForEach(entry, frames) {
        const char *name = cJSON_GetStringValue(cJSON_GetObjectItemCaseSensitive(entry, "name"));
        const char *hex = cJSON_GetStringValue(cJSON_GetObjectItemCaseSensitive(entry, "hex"));
        TEST_ASSERT_NOT_NULL(hex);
        size_t len = unhex(hex, s_wire, sizeof s_wire);
        td_frame_t frame;
        td_frame_result_t result = td_frame_decode_slip(s_wire, len, s_body, sizeof s_body, &frame);
        if (!cJSON_IsTrue(cJSON_GetObjectItemCaseSensitive(entry, "valid"))) {
            TEST_ASSERT_NOT_EQUAL_MESSAGE(TD_FRAME_OK, result, name);
            continue;
        }
        TEST_ASSERT_EQUAL_MESSAGE(TD_FRAME_OK, result, name);
        const cJSON *decoded = cJSON_GetObjectItemCaseSensitive(entry, "decoded");
        TEST_ASSERT_EQUAL_UINT8(number(decoded, "ver"), TD_FRAME_VERSION);
        TEST_ASSERT_EQUAL_UINT8(number(decoded, "type"), frame.type);
        TEST_ASSERT_EQUAL_UINT8(number(decoded, "seq"), frame.seq);
        TEST_ASSERT_EQUAL_UINT8(number(decoded, "flags"), frame.flags);
        TEST_ASSERT_EQUAL_UINT16(number(decoded, "len"), frame.len);
        size_t payload_len = unhex(cJSON_GetStringValue(cJSON_GetObjectItemCaseSensitive(decoded, "payload_hex")),
                                  s_payload, sizeof s_payload);
        TEST_ASSERT_EQUAL_UINT(payload_len, frame.len);
        if (frame.len) {
            TEST_ASSERT_EQUAL_MEMORY(s_payload, frame.payload, frame.len);
        }
        td_frame_t expected = {
            .type = (uint8_t)number(decoded, "type"), .seq = (uint8_t)number(decoded, "seq"),
            .flags = (uint8_t)number(decoded, "flags"), .len = (uint16_t)payload_len, .payload = s_payload,
        };
        size_t encoded_len = td_frame_encode(&expected, s_encoded, sizeof s_encoded);
        TEST_ASSERT_EQUAL_UINT_MESSAGE(len, encoded_len, name);
        TEST_ASSERT_EQUAL_MEMORY_MESSAGE(s_wire, s_encoded, len, name);
    }
    cJSON_Delete(root);
}

TEST_CASE("encoding rejects 1025 bytes and cannot write past its output capacity", "[td_frame]")
{
    td_frame_t frame = { .type = TD_FRAME_CHUNK, .seq = 1, .len = 1025, .payload = s_payload };
    TEST_ASSERT_EQUAL_UINT(0, td_frame_encode(&frame, s_wire, sizeof s_wire));
    frame.len = 1024;
    memset(s_payload, 0xc0, sizeof s_payload);
    size_t needed = td_frame_encode(&frame, s_wire, sizeof s_wire);
    TEST_ASSERT_GREATER_THAN_UINT(1100, needed);
    memset(s_encoded, 0xa5, sizeof s_encoded);
    TEST_ASSERT_EQUAL_UINT(0, td_frame_encode(&frame, s_encoded, needed - 1));
    TEST_ASSERT_EACH_EQUAL_HEX8(0xa5, s_encoded, sizeof s_encoded);
    TEST_ASSERT_EQUAL_UINT(needed, td_frame_encode(&frame, s_encoded, needed));
    td_frame_t decoded;
    TEST_ASSERT_EQUAL(TD_FRAME_OK, td_frame_decode_slip(s_encoded, needed, s_body, sizeof s_body, &decoded));
    TEST_ASSERT_EQUAL_MEMORY(s_payload, decoded.payload, 1024);
}

TEST_CASE("decode rejects a changed CRC version length and dangling escape", "[td_frame]")
{
    uint8_t body[] = {1, 1, 3, 0, 0, 0, 0, 0};
    uint16_t crc = td_crc16(body, 6);
    body[6] = crc & 0xff;
    body[7] = crc >> 8;
    td_frame_t frame;
    TEST_ASSERT_EQUAL(TD_FRAME_OK, td_frame_decode(body, sizeof body, &frame));
    body[7] ^= 1;
    TEST_ASSERT_EQUAL(TD_FRAME_BAD_CRC, td_frame_decode(body, sizeof body, &frame));
    body[0] = 2;
    TEST_ASSERT_EQUAL(TD_FRAME_BAD_VERSION, td_frame_decode(body, sizeof body, &frame));
    body[0] = 1;
    TEST_ASSERT_EQUAL(TD_FRAME_BAD_LENGTH, td_frame_decode(body, 7, &frame));
    body[4] = 1;
    body[5] = 4;
    TEST_ASSERT_EQUAL(TD_FRAME_BAD_LENGTH, td_frame_decode(body, sizeof body, &frame));
    const uint8_t dangling[] = {0xc0, 1, 0xdb, 0xc0};
    TEST_ASSERT_EQUAL(TD_FRAME_BAD_ESCAPE,
                      td_frame_decode_slip(dangling, sizeof dangling, s_body, sizeof s_body, &frame));
}

TEST_CASE("a maximum chunk carries 1021 data bytes even when every payload byte needs escaping", "[td_frame]")
{
    TEST_ASSERT_EQUAL_UINT(1021, TD_FRAME_CHUNK_DATA_MAX);
    TEST_ASSERT_EQUAL_UINT(1032, TD_FRAME_BODY_MAX);
    TEST_ASSERT_EQUAL_UINT(2066, TD_FRAME_WIRE_MAX);
    for (size_t i = 0; i < TD_FRAME_PAYLOAD_MAX; i++) {
        s_payload[i] = i % 2 ? 0xdb : 0xc0;
    }
    td_frame_t frame = {
        .type = TD_FRAME_CHUNK, .seq = 0xdb, .len = TD_FRAME_PAYLOAD_MAX, .payload = s_payload,
    };
    size_t wire_len = td_frame_encode(&frame, s_wire, sizeof s_wire);
    TEST_ASSERT_GREATER_THAN_UINT(2 * TD_FRAME_PAYLOAD_MAX, wire_len);
    TEST_ASSERT_LESS_OR_EQUAL_UINT(TD_FRAME_WIRE_MAX, wire_len);
    td_frame_t decoded;
    TEST_ASSERT_EQUAL(TD_FRAME_OK, td_frame_decode_slip(s_wire, wire_len, s_body, sizeof s_body, &decoded));
    TEST_ASSERT_EQUAL_UINT(1024, decoded.len);
    TEST_ASSERT_EQUAL_MEMORY(s_payload, decoded.payload, TD_FRAME_PAYLOAD_MAX);
    frame.len = TD_FRAME_CHUNK_HEADER;
    wire_len = td_frame_encode(&frame, s_wire, sizeof s_wire);
    TEST_ASSERT_EQUAL(TD_FRAME_OK, td_frame_decode_slip(s_wire, wire_len, s_body, sizeof s_body, &decoded));
    TEST_ASSERT_EQUAL_UINT(3, decoded.len);
    frame.len--;
    TEST_ASSERT_EQUAL_UINT(0, td_frame_encode(&frame, s_wire, sizeof s_wire));
}
