#include <string.h>
#include "td_xfer.h"
#include "unity.h"

#define DATA_BYTES 3000 /* three chunks: 1021, 1021, 958 */

static uint8_t s_data[DATA_BYTES];
static uint8_t s_out[DATA_BYTES + 16];
static size_t s_out_len;
static unsigned s_sink_calls;

static bool fake_sink(void *ctx, const uint8_t *data, size_t len)
{
    (void)ctx;
    memcpy(s_out + s_out_len, data, len);
    s_out_len += len;
    s_sink_calls++;
    return true;
}

static void fill_data(void)
{
    for (size_t i = 0; i < DATA_BYTES; i++) {
        s_data[i] = (uint8_t)(i * 7 + 3);
    }
    memset(s_out, 0, sizeof s_out);
    s_out_len = 0;
    s_sink_calls = 0;
}

static const uint8_t *chunk_data(unsigned idx, size_t *len)
{
    size_t start = (size_t)idx * TD_FRAME_CHUNK_DATA_MAX;
    *len = DATA_BYTES - start < TD_FRAME_CHUNK_DATA_MAX ? DATA_BYTES - start : TD_FRAME_CHUNK_DATA_MAX;
    return s_data + start;
}

static bool deliver(td_xfer_t *rx, unsigned idx)
{
    size_t len;
    const uint8_t *data = chunk_data(idx, &len);
    return td_xfer_receive(rx, (uint16_t)idx, data, len);
}

TEST_CASE("the crc-32 check value", "[td_xfer]")
{
    TEST_ASSERT_EQUAL_HEX32(0xcbf43926, td_xfer_crc32((const uint8_t *)"123456789", 9));
    TEST_ASSERT_EQUAL_HEX32(0, td_xfer_crc32(NULL, 0));
}

TEST_CASE("an in-order transfer reaches the sink whole and completes", "[td_xfer]")
{
    fill_data();
    td_xfer_t rx;
    td_xfer_init(&rx, 7, false, DATA_BYTES, td_xfer_crc32(s_data, DATA_BYTES), fake_sink, NULL);
    TEST_ASSERT_FALSE(td_xfer_complete(&rx));
    for (unsigned idx = 0; idx < 3; idx++) {
        TEST_ASSERT_TRUE(deliver(&rx, idx));
        TEST_ASSERT_EQUAL_UINT16(idx + 1, rx.next);
    }
    TEST_ASSERT_TRUE(td_xfer_complete(&rx));
    TEST_ASSERT_EQUAL_UINT(DATA_BYTES, s_out_len);
    TEST_ASSERT_EQUAL_MEMORY(s_data, s_out, DATA_BYTES);
}

TEST_CASE("a lost chunk goes again after 200 ms and the sender finishes", "[td_xfer]")
{
    fill_data();
    td_xfer_t tx;
    td_xfer_init(&tx, 1, true, DATA_BYTES, td_xfer_crc32(s_data, DATA_BYTES), NULL, NULL);
    TEST_ASSERT_EQUAL_INT(0, td_xfer_due(&tx, 1000));
    TEST_ASSERT_EQUAL_INT(1, td_xfer_due(&tx, 1000));
    TEST_ASSERT_EQUAL_INT(2, td_xfer_due(&tx, 1000));
    TEST_ASSERT_EQUAL_INT(-1, td_xfer_due(&tx, 1000));
    /* Chunk 0 arrives, so the receiver acks 1. Chunk 1 is lost. */
    td_xfer_ack(&tx, 1);
    TEST_ASSERT_EQUAL_INT(-1, td_xfer_due(&tx, 1199));
    TEST_ASSERT_EQUAL_INT(1, td_xfer_due(&tx, 1200));
    TEST_ASSERT_EQUAL_INT(2, td_xfer_due(&tx, 1200));
    td_xfer_ack(&tx, 3);
    TEST_ASSERT_TRUE(td_xfer_complete(&tx));
    TEST_ASSERT_FALSE(tx.aborted);
}

TEST_CASE("a duplicate chunk is acked and not applied twice", "[td_xfer]")
{
    fill_data();
    td_xfer_t rx;
    td_xfer_init(&rx, 7, false, DATA_BYTES, td_xfer_crc32(s_data, DATA_BYTES), fake_sink, NULL);
    TEST_ASSERT_TRUE(deliver(&rx, 0));
    TEST_ASSERT_TRUE(deliver(&rx, 0));
    TEST_ASSERT_EQUAL_UINT(1, s_sink_calls);
    TEST_ASSERT_EQUAL_UINT16(1, rx.next);
    TEST_ASSERT_TRUE(deliver(&rx, 1));
    TEST_ASSERT_TRUE(deliver(&rx, 0));
    TEST_ASSERT_EQUAL_UINT(2, s_sink_calls);
    TEST_ASSERT_EQUAL_UINT16(2, rx.next);
}

TEST_CASE("a chunk past the window or past a gap is refused and changes nothing", "[td_xfer]")
{
    fill_data();
    td_xfer_t rx;
    td_xfer_init(&rx, 7, false, DATA_BYTES, td_xfer_crc32(s_data, DATA_BYTES), fake_sink, NULL);
    TEST_ASSERT_FALSE(deliver(&rx, 2));
    TEST_ASSERT_FALSE(td_xfer_receive(&rx, TD_XFER_WINDOW, s_data, TD_FRAME_CHUNK_DATA_MAX));
    TEST_ASSERT_FALSE(td_xfer_receive(&rx, 0, s_data, TD_FRAME_CHUNK_DATA_MAX + 1));
    TEST_ASSERT_EQUAL_UINT(0, s_sink_calls);
    TEST_ASSERT_EQUAL_UINT16(0, rx.next);
    TEST_ASSERT_TRUE(deliver(&rx, 0));
}

TEST_CASE("a crc mismatch at the end leaves the transfer incomplete", "[td_xfer]")
{
    fill_data();
    td_xfer_t rx;
    td_xfer_init(&rx, 7, false, DATA_BYTES, td_xfer_crc32(s_data, DATA_BYTES) ^ 1, fake_sink, NULL);
    for (unsigned idx = 0; idx < 3; idx++) {
        TEST_ASSERT_TRUE(deliver(&rx, idx));
    }
    TEST_ASSERT_EQUAL_UINT(DATA_BYTES, rx.received);
    TEST_ASSERT_FALSE(td_xfer_complete(&rx));
}

TEST_CASE("five transmissions without an ack abort the transfer", "[td_xfer]")
{
    fill_data();
    td_xfer_t tx;
    td_xfer_init(&tx, 1, true, DATA_BYTES, 0, NULL, NULL);
    uint32_t now = 1000;
    TEST_ASSERT_EQUAL_INT(0, td_xfer_due(&tx, now));
    TEST_ASSERT_EQUAL_INT(1, td_xfer_due(&tx, now));
    TEST_ASSERT_EQUAL_INT(2, td_xfer_due(&tx, now));
    for (int again = 2; again <= TD_XFER_TRANSMISSIONS; again++) {
        now += TD_XFER_RETRY_MS;
        TEST_ASSERT_EQUAL_INT(0, td_xfer_due(&tx, now));
        TEST_ASSERT_EQUAL_INT(1, td_xfer_due(&tx, now));
        TEST_ASSERT_EQUAL_INT(2, td_xfer_due(&tx, now));
    }
    TEST_ASSERT_FALSE(tx.aborted);
    now += TD_XFER_RETRY_MS;
    TEST_ASSERT_EQUAL_INT(-1, td_xfer_due(&tx, now));
    TEST_ASSERT_TRUE(tx.aborted);
    TEST_ASSERT_FALSE(td_xfer_complete(&tx));
}
