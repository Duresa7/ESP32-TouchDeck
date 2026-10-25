#include <stdio.h>
#include <string.h>
#include "td_update_state.h"
#include "unity.h"

#define SLOT_BYTES 0x400000u /* a 4 MB OTA slot */
#define IMAGE_BYTES 3000     /* three chunks: 1021, 1021, 958 */
#define CHUNK 1021

static uint8_t s_image[IMAGE_BYTES];
static char s_hex[TD_SHA256_HEX + 1];

static void to_hex(const uint8_t digest[TD_SHA256_BYTES], char out[TD_SHA256_HEX + 1])
{
    for (int i = 0; i < TD_SHA256_BYTES; i++) {
        snprintf(out + 2 * i, 3, "%02x", digest[i]);
    }
}

static void digest_hex(const uint8_t *data, size_t len, char out[TD_SHA256_HEX + 1])
{
    td_sha256_t sha;
    uint8_t digest[TD_SHA256_BYTES];
    td_sha256_init(&sha);
    td_sha256_update(&sha, data, len);
    td_sha256_final(&sha, digest);
    to_hex(digest, out);
}

static void prepare_image(void)
{
    for (size_t i = 0; i < IMAGE_BYTES; i++) {
        s_image[i] = (uint8_t)(i * 13 + 5);
    }
    digest_hex(s_image, IMAGE_BYTES, s_hex);
}

static size_t chunk_len(unsigned idx)
{
    const size_t start = (size_t)idx * CHUNK;
    return IMAGE_BYTES - start < CHUNK ? IMAGE_BYTES - start : CHUNK;
}

static void send_all(td_update_state_t *u)
{
    for (unsigned idx = 0; idx < 3; idx++) {
        TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_chunk(u, s_image + (size_t)idx * CHUNK, chunk_len(idx)));
    }
}

TEST_CASE("sha-256 matches the published vectors", "[td_update]")
{
    static const struct {
        const char *text;
        const char *hex;
    } vectors[] = {
        { "", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" },
        { "abc", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" },
        { "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
          "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1" },
    };
    char hex[TD_SHA256_HEX + 1];
    for (size_t i = 0; i < sizeof vectors / sizeof vectors[0]; i++) {
        digest_hex((const uint8_t *)vectors[i].text, strlen(vectors[i].text), hex);
        TEST_ASSERT_EQUAL_STRING(vectors[i].hex, hex);
    }
}

TEST_CASE("sha-256 of a million a's, fed in thousand-byte pieces", "[td_update]")
{
    uint8_t piece[1000];
    memset(piece, 'a', sizeof piece);
    td_sha256_t sha;
    uint8_t digest[TD_SHA256_BYTES];
    char hex[TD_SHA256_HEX + 1];
    td_sha256_init(&sha);
    for (int i = 0; i < 1000; i++) {
        td_sha256_update(&sha, piece, sizeof piece);
    }
    td_sha256_final(&sha, digest);
    to_hex(digest, hex);
    TEST_ASSERT_EQUAL_STRING("cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0", hex);
}

TEST_CASE("sha-256 of an image fed in chunk-sized pieces equals the whole", "[td_update]")
{
    prepare_image();
    td_sha256_t sha;
    uint8_t digest[TD_SHA256_BYTES];
    char hex[TD_SHA256_HEX + 1];
    td_sha256_init(&sha);
    for (unsigned idx = 0; idx < 3; idx++) {
        td_sha256_update(&sha, s_image + (size_t)idx * CHUNK, chunk_len(idx));
    }
    td_sha256_final(&sha, digest);
    to_hex(digest, hex);
    TEST_ASSERT_EQUAL_STRING(s_hex, hex);
}

TEST_CASE("a hash is exactly 64 hex digits of either case", "[td_update]")
{
    uint8_t out[TD_SHA256_BYTES];
    TEST_ASSERT_TRUE(td_sha256_from_hex("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", out));
    TEST_ASSERT_EQUAL_HEX8(0xba, out[0]);
    TEST_ASSERT_EQUAL_HEX8(0xad, out[31]);
    TEST_ASSERT_TRUE(td_sha256_from_hex("BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD", out));
    TEST_ASSERT_EQUAL_HEX8(0xba, out[0]);
    TEST_ASSERT_FALSE(td_sha256_from_hex("ba7816bf", out));
    TEST_ASSERT_FALSE(td_sha256_from_hex("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015a", out));
    TEST_ASSERT_FALSE(td_sha256_from_hex("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015adz", out));
    TEST_ASSERT_FALSE(td_sha256_from_hex("za7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", out));
    TEST_ASSERT_FALSE(td_sha256_from_hex(NULL, out));
}

TEST_CASE("a full update goes IDLE, RECEIVING, VERIFYING, REBOOTING", "[td_update]")
{
    prepare_image();
    td_update_state_t u;
    td_update_state_init(&u);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_IDLE, u.phase);

    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_begin(&u, IMAGE_BYTES, s_hex, SLOT_BYTES, true));
    td_update_state_started(&u, 42);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_RECEIVING, u.phase);
    TEST_ASSERT_EQUAL_UINT32(42, u.handle);
    TEST_ASSERT_EQUAL_UINT32(IMAGE_BYTES, u.bytes);
    TEST_ASSERT_EQUAL_UINT32(0, u.written);

    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_chunk(&u, s_image, CHUNK));
    TEST_ASSERT_EQUAL_UINT32(CHUNK, u.written);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_chunk(&u, s_image + CHUNK, CHUNK));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_chunk(&u, s_image + 2 * CHUNK, chunk_len(2)));
    TEST_ASSERT_EQUAL_UINT32(IMAGE_BYTES, u.written);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_RECEIVING, u.phase);

    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_finish(&u));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_VERIFYING, u.phase);
    td_update_state_rebooting(&u);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_REBOOTING, u.phase);
    TEST_ASSERT_EQUAL_STRING("REBOOTING", td_update_phase_name(u.phase));

    /* Nothing starts another update once the deck is restarting. */
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_E_BUSY, td_update_state_begin(&u, IMAGE_BYTES, s_hex, SLOT_BYTES, true));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_REBOOTING, u.phase);
}

TEST_CASE("an image over the slot is refused, and the slot size itself is not", "[td_update]")
{
    prepare_image();
    td_update_state_t u;
    td_update_state_init(&u);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_E_SIZE, td_update_state_begin(&u, SLOT_BYTES + 1, s_hex, SLOT_BYTES, true));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_IDLE, u.phase);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_E_SIZE, td_update_state_begin(&u, 0, s_hex, SLOT_BYTES, true));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_IDLE, u.phase);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_begin(&u, SLOT_BYTES, s_hex, SLOT_BYTES, true));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_RECEIVING, u.phase);
}

TEST_CASE("a hash that is not 64 hex digits is refused", "[td_update]")
{
    td_update_state_t u;
    td_update_state_init(&u);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_E_SHA_FORMAT, td_update_state_begin(&u, IMAGE_BYTES, "abc", SLOT_BYTES, true));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_IDLE, u.phase);
}

TEST_CASE("a chunk before begin is refused and changes nothing", "[td_update]")
{
    prepare_image();
    td_update_state_t u;
    td_update_state_init(&u);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_E_STATE, td_update_state_chunk(&u, s_image, CHUNK));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_IDLE, u.phase);
    TEST_ASSERT_EQUAL_UINT32(0, u.written);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_E_STATE, td_update_state_finish(&u));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_IDLE, u.phase);
}

TEST_CASE("a hash mismatch at the end fails the update and a new one may follow", "[td_update]")
{
    prepare_image();
    char wrong[TD_SHA256_HEX + 1];
    memcpy(wrong, s_hex, sizeof wrong);
    wrong[0] = wrong[0] == '0' ? '1' : '0';

    td_update_state_t u;
    td_update_state_init(&u);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_begin(&u, IMAGE_BYTES, wrong, SLOT_BYTES, true));
    send_all(&u);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_E_SHA, td_update_state_finish(&u));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_FAILED, u.phase);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_E_SHA, u.reason);
    TEST_ASSERT_EQUAL_STRING("sha256", td_update_reason_name(u.reason));

    /* It never reached VERIFYING, so the boot slot was never touched. A chunk now has nowhere to go. */
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_E_STATE, td_update_state_chunk(&u, s_image, 1));

    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_begin(&u, IMAGE_BYTES, s_hex, SLOT_BYTES, true));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_RECEIVING, u.phase);
    TEST_ASSERT_EQUAL_UINT32(0, u.written);
    send_all(&u);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_finish(&u));
}

TEST_CASE("a second begin during RECEIVING is refused and the update goes on", "[td_update]")
{
    prepare_image();
    td_update_state_t u;
    td_update_state_init(&u);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_begin(&u, IMAGE_BYTES, s_hex, SLOT_BYTES, true));
    td_update_state_started(&u, 9);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_chunk(&u, s_image, CHUNK));

    TEST_ASSERT_EQUAL_INT(TD_UPDATE_E_BUSY, td_update_state_begin(&u, 500, s_hex, SLOT_BYTES, true));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_RECEIVING, u.phase);
    TEST_ASSERT_EQUAL_UINT32(IMAGE_BYTES, u.bytes);
    TEST_ASSERT_EQUAL_UINT32(CHUNK, u.written);
    TEST_ASSERT_EQUAL_UINT32(9, u.handle);

    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_chunk(&u, s_image + CHUNK, CHUNK));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_chunk(&u, s_image + 2 * CHUNK, chunk_len(2)));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_finish(&u));
}

TEST_CASE("a chunk past the declared size fails the update", "[td_update]")
{
    prepare_image();
    td_update_state_t u;
    td_update_state_init(&u);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_begin(&u, 1500, s_hex, SLOT_BYTES, true));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_chunk(&u, s_image, CHUNK));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_E_RANGE, td_update_state_chunk(&u, s_image + CHUNK, CHUNK));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_FAILED, u.phase);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_E_RANGE, u.reason);
}

TEST_CASE("an end before every byte arrived fails with incomplete", "[td_update]")
{
    prepare_image();
    td_update_state_t u;
    td_update_state_init(&u);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_begin(&u, IMAGE_BYTES, s_hex, SLOT_BYTES, true));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_chunk(&u, s_image, CHUNK));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_E_INCOMPLETE, td_update_state_finish(&u));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_FAILED, u.phase);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_E_INCOMPLETE, u.reason);
}

TEST_CASE("a cut transfer returns the machine to IDLE", "[td_update]")
{
    prepare_image();
    td_update_state_t u;
    td_update_state_init(&u);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_begin(&u, IMAGE_BYTES, s_hex, SLOT_BYTES, true));
    td_update_state_started(&u, 3);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_chunk(&u, s_image, CHUNK));
    td_update_state_abort(&u);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_IDLE, u.phase);
    TEST_ASSERT_EQUAL_UINT32(0, u.handle);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_begin(&u, IMAGE_BYTES, s_hex, SLOT_BYTES, true));
    TEST_ASSERT_EQUAL_UINT32(0, u.written);
}

TEST_CASE("an image that has not confirmed itself refuses an update", "[td_update]")
{
    prepare_image();
    td_update_state_t u;
    td_update_state_init(&u);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_E_UNCONFIRMED, td_update_state_begin(&u, IMAGE_BYTES, s_hex, SLOT_BYTES, false));
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_IDLE, u.phase);
    TEST_ASSERT_EQUAL_INT(TD_UPDATE_OK, td_update_state_begin(&u, IMAGE_BYTES, s_hex, SLOT_BYTES, true));
}

TEST_CASE("confirming needs the first frame, a layout and ten seconds, all three", "[td_update]")
{
    for (unsigned mask = 0; mask < 8; mask++) {
        const td_update_confirm_inputs_t in = {
            .first_frame = mask & 1,
            .layout_loaded = mask & 2,
            .uptime_ms = (mask & 4) ? TD_UPDATE_CONFIRM_UPTIME_MS : TD_UPDATE_CONFIRM_UPTIME_MS - 1,
        };
        TEST_ASSERT_EQUAL_INT_MESSAGE(mask == 7, td_update_confirm_ready(&in), "only all three confirm");
    }
    const td_update_confirm_inputs_t long_up = { .first_frame = true, .layout_loaded = true, .uptime_ms = 3600000 };
    TEST_ASSERT_TRUE(td_update_confirm_ready(&long_up));
    const td_update_confirm_inputs_t fresh = { .first_frame = true, .layout_loaded = true, .uptime_ms = 0 };
    TEST_ASSERT_FALSE(td_update_confirm_ready(&fresh));
}
