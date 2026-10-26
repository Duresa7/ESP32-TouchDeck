#include <stdint.h>
#include <string.h>
#include "unity.h"
#include "td_frame.h"
#include "td_rx_sync.h"

#define QUEUE_MAX 2048
#define LOG_MAX 8192
#define RAN_MAX 16384

static struct {
    td_rx_sync_t sync;
    td_rx_split_t split;
    size_t space;
    char ring[QUEUE_MAX];
    size_t cap;
    size_t rd;
    size_t used;
    size_t dropped;
    size_t overflows;
    bool queued_last_read;
    char queued[LOG_MAX];
    size_t queued_len;
    char line[64];
    td_line_t line_state;
    uint32_t now;
    uint8_t frame[TD_RX_FRAME_MAX];
    size_t frame_len;
    size_t frames;
    uint8_t seq;
    size_t refused;
    size_t frame_after_text;
    size_t bad_frames;
    char ran[RAN_MAX];
    size_t ran_len;
} s_model;

static void frame_received(const uint8_t *body, size_t len, void *context)
{
    TEST_ASSERT_EQUAL_PTR(&s_model, context);
    td_frame_t f;
    if (td_frame_decode(body, len, &f) != TD_FRAME_OK) {
        s_model.bad_frames++;
        return;
    }
    memcpy(s_model.frame, body, len);
    s_model.frame_len = len;
    s_model.frames++;
    s_model.seq = f.seq;
    s_model.frame_after_text = s_model.queued_len;
}

static void model_reset(size_t cap)
{
    memset(&s_model, 0, sizeof s_model);
    s_model.cap = cap;
    s_model.queued_last_read = true;
}

static void text_received(char *buf, size_t n, void *context)
{
    TEST_ASSERT_EQUAL_PTR(&s_model, context);
    size_t dropped_before = s_model.dropped;
    size_t m = td_rx_sync(&s_model.sync, buf, n, s_model.space, &s_model.dropped);
    if (s_model.dropped > dropped_before && s_model.queued_last_read) {
        s_model.overflows++;
    }
    s_model.queued_last_read = m > 0;
    TEST_ASSERT_TRUE_MESSAGE(m <= s_model.space, "queued more than the free space");
    s_model.space -= m;
    for (size_t i = 0; i < m; i++) {
        s_model.ring[(s_model.rd + s_model.used) % QUEUE_MAX] = buf[i];
        s_model.used++;
        TEST_ASSERT_TRUE(s_model.queued_len < LOG_MAX);
        s_model.queued[s_model.queued_len++] = buf[i];
    }
}

static size_t feed_with_space(const char *s, size_t n, size_t space)
{
    char buf[64];
    TEST_ASSERT_TRUE(n <= sizeof buf);
    memcpy(buf, s, n);
    s_model.space = space;
    td_rx_split(&s_model.split, s_model.now, buf, n, text_received, frame_received, &s_model);
    return space - s_model.space;
}

static size_t feed(const char *s, size_t n)
{
    return feed_with_space(s, n, s_model.cap - s_model.used);
}

static void feed_bytes(const uint8_t *p, size_t n)
{
    for (size_t off = 0; off < n; off += 64) {
        feed((const char *)p + off, n - off < 64 ? n - off : 64);
    }
}

static uint32_t at(uint32_t now)
{
    s_model.now = now;
    return td_rx_split_expire(&s_model.split, now, frame_received, &s_model);
}

static void console_byte(char c)
{
    char echo[3];
    size_t echo_len;
    td_line_status_t status = td_line_feed(&s_model.line_state, (unsigned char)c, false,
                                          s_model.line, sizeof s_model.line, echo, &echo_len);
    TEST_ASSERT_NOT_EQUAL(TD_LINE_TOO_LONG, status);
    s_model.refused += status == TD_LINE_CONTROL;
    if (status == TD_LINE_OK && s_model.line[0]) {
        size_t len = strlen(s_model.line);
        TEST_ASSERT_TRUE(s_model.ran_len + len + 1 < RAN_MAX);
        memcpy(s_model.ran + s_model.ran_len, s_model.line, len);
        s_model.ran_len += len;
        s_model.ran[s_model.ran_len++] = '\n';
        s_model.ran[s_model.ran_len] = '\0';
    }
}

static void drain(size_t max)
{
    while (max-- > 0 && s_model.used > 0) {
        char c = s_model.ring[s_model.rd];
        s_model.rd = (s_model.rd + 1) % QUEUE_MAX;
        s_model.used--;
        console_byte(c);
    }
}

static size_t lines_ran(void)
{
    size_t lines = 0;
    for (size_t i = 0; i < s_model.ran_len; i++) {
        lines += s_model.ran[i] == '\n';
    }
    return lines;
}

/* The sum a script computes for CR LF input: each line that ran counts its text and two ending bytes, and the drops add their counted bytes. */
static size_t accounted_crlf(void)
{
    return s_model.ran_len + lines_ran() + s_model.dropped;
}

#define FEED(str, space) feed_with_space(str, sizeof(str) - 1, space)
#define EXPECT_QUEUED(str) do { \
        TEST_ASSERT_EQUAL_UINT(sizeof(str) - 1, s_model.queued_len); \
        TEST_ASSERT_EQUAL_MEMORY(str, s_model.queued, sizeof(str) - 1); \
    } while (0)

TEST_CASE("a read that fits is queued unchanged", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    TEST_ASSERT_EQUAL_UINT(8, FEED("version\n", 100));
    drain(QUEUE_MAX);
    EXPECT_QUEUED("version\n");
    TEST_ASSERT_EQUAL_STRING("version\n", s_model.ran);
    TEST_ASSERT_EQUAL_UINT(0, s_model.dropped);
}

TEST_CASE("a drop mid-line abandons the queued head and skips the rest of the line", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("he", 100);
    FEED("ap\nhea", 6);
    FEED("p\nversion\n", 100);
    drain(QUEUE_MAX);
    EXPECT_QUEUED("he\0version\n");
    TEST_ASSERT_EQUAL_STRING("version\n", s_model.ran);
    TEST_ASSERT_EQUAL_UINT(2 + 6 + 2, s_model.dropped);
}

TEST_CASE("a drop that ends on a newline leaves the next line intact", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("he", 100);
    FEED("ap\n", 3);
    FEED("version\n", 100);
    drain(QUEUE_MAX);
    EXPECT_QUEUED("he\0version\n");
    TEST_ASSERT_EQUAL_STRING("version\n", s_model.ran);
    TEST_ASSERT_EQUAL_UINT(2 + 3, s_model.dropped);
}

TEST_CASE("a drop at a line start queues no abandon marker", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("heap\nhe", 7);
    FEED("ap\nversion\n", 100);
    drain(QUEUE_MAX);
    EXPECT_QUEUED("version\n");
    TEST_ASSERT_EQUAL_STRING("version\n", s_model.ran);
    TEST_ASSERT_EQUAL_UINT(7 + 3, s_model.dropped);
}

TEST_CASE("a drop followed by complete lines runs them and accounts for every byte", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    const char *sent = "heap\nheap\nheap\nversion\n";
    FEED("hea", 100);
    FEED("p\nhe", 4);
    FEED("ap\nheap\nversion\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_STRING("heap\nversion\n", s_model.ran);
    TEST_ASSERT_EQUAL_UINT(3 + 4 + 3, s_model.dropped);
    TEST_ASSERT_EQUAL_UINT(strlen(sent), s_model.ran_len + s_model.dropped);
}

TEST_CASE("input stays discarded across reads until a newline arrives", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("he", 100);
    FEED("apxxxxxx", 8);
    FEED("yyyyyyyy", 100);
    FEED("zz\nversion\n", 100);
    drain(QUEUE_MAX);
    EXPECT_QUEUED("he\0version\n");
    TEST_ASSERT_EQUAL_STRING("version\n", s_model.ran);
    TEST_ASSERT_EQUAL_UINT(2 + 8 + 8 + 3, s_model.dropped);
}

TEST_CASE("a carriage return ends the discarded line and its line feed starts an empty one", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("he", 100);
    FEED("ap\r\nhe", 6);
    FEED("ap\r\nheap\r\n", 100);
    drain(QUEUE_MAX);
    EXPECT_QUEUED("he\0\nheap\r\n");
    TEST_ASSERT_EQUAL_STRING("heap\n", s_model.ran);
    TEST_ASSERT_EQUAL_UINT(2 + 6 + 3, s_model.dropped);
}

TEST_CASE("a drop that ends on a carriage return keeps the line feed that follows", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("he", 100);
    FEED("ap\r", 3);
    FEED("\nheap\r\n", 100);
    drain(QUEUE_MAX);
    EXPECT_QUEUED("he\0\nheap\r\n");
    TEST_ASSERT_EQUAL_STRING("heap\n", s_model.ran);
    TEST_ASSERT_EQUAL_UINT(2 + 3, s_model.dropped);
}

TEST_CASE("carriage return line feed input can leave the byte accounting one short", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("he", 100);
    FEED("ap\r", 3);
    FEED("\nheap\r\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_STRING("heap\n", s_model.ran);
    TEST_ASSERT_EQUAL_UINT(sizeof("heap\r\nheap\r\n") - 1 - 1, accounted_crlf());
}

TEST_CASE("carriage return line feed input can count the line feed of an answered line twice", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("heap\r", 100);
    FEED("\n", 1);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_STRING("heap\n", s_model.ran);
    TEST_ASSERT_EQUAL_UINT(sizeof("heap\r\n") - 1 + 1, accounted_crlf());
}

TEST_CASE("a read is queued only when one byte stays free", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    TEST_ASSERT_EQUAL_UINT(0, FEED("version\n", 8));
    TEST_ASSERT_EQUAL_UINT(8, s_model.dropped);
    TEST_ASSERT_EQUAL_UINT(8, FEED("version\n", 9));
    TEST_ASSERT_EQUAL_UINT(8, s_model.dropped);
}

TEST_CASE("the abandon marker fits in a queue that is otherwise full", "[td_rx_sync]")
{
    model_reset(8);
    TEST_ASSERT_EQUAL_UINT(7, feed("aaaaaaa", 7));
    TEST_ASSERT_EQUAL_UINT(1, feed("b", 1));
    TEST_ASSERT_EQUAL_UINT(8, s_model.used);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_STRING("", s_model.ran);
    TEST_ASSERT_EQUAL_UINT(7 + 1, s_model.dropped);
}

TEST_CASE("a nul byte from the host is queued as a question mark", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("a\0b\n", 100);
    drain(QUEUE_MAX);
    EXPECT_QUEUED("a?b\n");
    TEST_ASSERT_EQUAL_STRING("a?b\n", s_model.ran);
    TEST_ASSERT_EQUAL_UINT(0, s_model.dropped);
}

TEST_CASE("1000 heap lines in 64 byte reads run only whole lines", "[td_rx_sync]")
{
    static char stream[5000];
    for (size_t i = 0; i < 1000; i++) {
        memcpy(stream + 5 * i, "heap\n", 5);
    }
    model_reset(QUEUE_MAX);
    for (size_t off = 0; off < sizeof stream; off += 64) {
        size_t n = sizeof stream - off < 64 ? sizeof stream - off : 64;
        feed(stream + off, n);
    }
    drain(QUEUE_MAX);
    TEST_ASSERT_TRUE(s_model.dropped > 0);
    TEST_ASSERT_EQUAL_UINT(0, s_model.ran_len % 5);
    for (size_t i = 0; i < s_model.ran_len; i += 5) {
        TEST_ASSERT_EQUAL_MEMORY("heap\n", s_model.ran + i, 5);
    }
    TEST_ASSERT_EQUAL_UINT(sizeof stream, s_model.ran_len + s_model.dropped);
}

#define FUZZ_LINES 1000
#define FUZZ_TOKEN_MAX 8

static uint32_t rnd(uint32_t *x)
{
    *x = *x * 1664525u + 1013904223u;
    return *x >> 16;
}

/*
 * Lines of 3, 4 or 6 random letters, sent in reads of 1 to 64 bytes while the
 * reader pops a few bytes per read, so the queue fills and drops often. Every
 * line that runs must be one that was sent, in order. A line glued from two
 * fragments is a random string that was never sent.
 */
static void fuzz(uint32_t seed, size_t cap, const char *eol)
{
    static char tokens[FUZZ_LINES][FUZZ_TOKEN_MAX];
    static char stream[FUZZ_LINES * (FUZZ_TOKEN_MAX + 1)];
    static const size_t token_len[3] = { 3, 4, 6 };
    uint32_t x = seed;
    size_t eol_len = strlen(eol);
    size_t stream_len = 0;
    for (size_t i = 0; i < FUZZ_LINES; i++) {
        size_t len = token_len[rnd(&x) % 3];
        for (size_t j = 0; j < len; j++) {
            tokens[i][j] = (char)('a' + rnd(&x) % 26);
        }
        tokens[i][len] = '\0';
        memcpy(stream + stream_len, tokens[i], len);
        memcpy(stream + stream_len + len, eol, eol_len);
        stream_len += len + eol_len;
    }

    model_reset(cap);
    for (size_t off = 0; off < stream_len;) {
        size_t n = 1 + rnd(&x) % 64;
        if (n > stream_len - off) {
            n = stream_len - off;
        }
        feed(stream + off, n);
        off += n;
        drain(rnd(&x) % 24);
    }
    drain(QUEUE_MAX);

    TEST_ASSERT_TRUE(s_model.dropped > 0);
    TEST_ASSERT_TRUE(s_model.ran_len > 0);
    TEST_ASSERT_EQUAL_UINT(0, s_model.line_state.len);

    size_t next = 0;
    size_t ran_bytes = 0;
    for (const char *p = s_model.ran; *p;) {
        const char *end = strchr(p, '\n');
        size_t len = (size_t)(end - p);
        while (next < FUZZ_LINES && !(strlen(tokens[next]) == len && memcmp(tokens[next], p, len) == 0)) {
            next++;
        }
        TEST_ASSERT_TRUE_MESSAGE(next < FUZZ_LINES, "ran a line that was not sent, or out of order");
        next++;
        ran_bytes += len + eol_len;
        p = end + 1;
    }
    size_t total = ran_bytes + s_model.dropped;
    if (eol_len == 1) {
        TEST_ASSERT_EQUAL_UINT(stream_len, total);
    } else {
        size_t gap = total > stream_len ? total - stream_len : stream_len - total;
        TEST_ASSERT_TRUE_MESSAGE(gap <= s_model.overflows, "off by more than one byte per queue overflow");
    }
}

TEST_CASE("random line lengths and read sizes never run a glued line", "[td_rx_sync]")
{
    fuzz(1, 2048, "\n");
    fuzz(2, 2048, "\n");
    fuzz(3, 512, "\n");
    fuzz(4, 128, "\n");
}

TEST_CASE("carriage return line feed input never runs a glued line", "[td_rx_sync]")
{
    fuzz(5, 2048, "\r\n");
    fuzz(6, 512, "\r\n");
    fuzz(7, 128, "\r\n");
}

TEST_CASE("carriage return input never runs a glued line and accounts for every byte", "[td_rx_sync]")
{
    fuzz(8, 2048, "\r");
    fuzz(9, 512, "\r");
    fuzz(10, 128, "\r");
}

#define FRAME "\xc0\x01\x01\x07\x00\x02\x00\xdb\xdc\xdb\xdd\xe7\x87\xc0"
#define FRAME_BODY "\x01\x01\x07\x00\x02\x00\xc0\xdb\xe7\x87"
#define BAD_CRC_FRAME "\xc0\x01\x01\x07\x00\x02\x00\xdb\xdc\xdb\xdd\xe7\x86\xc0"

static size_t message(uint8_t seq, const char *json, uint8_t *wire, size_t cap)
{
    td_frame_t f = { .type = TD_FRAME_MESSAGE, .seq = seq, .len = (uint16_t)strlen(json),
                     .payload = (const uint8_t *)json };
    size_t n = td_frame_encode(&f, wire, cap);
    TEST_ASSERT_NOT_EQUAL(0, n);
    return n;
}

TEST_CASE("a frame between text lines reaches only the frame receiver", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("heap\n" FRAME "version\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_STRING("heap\nversion\n", s_model.ran);
    TEST_ASSERT_EQUAL_UINT(1, s_model.frames);
    TEST_ASSERT_EQUAL_UINT(sizeof FRAME_BODY - 1, s_model.frame_len);
    TEST_ASSERT_EQUAL_MEMORY(FRAME_BODY, s_model.frame, sizeof FRAME_BODY - 1);
    TEST_ASSERT_EQUAL_UINT(0, s_model.dropped);
}

TEST_CASE("a frame and its escape survive separate USB reads", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("he\xc0\x01\x01\x07\x00\x02\x00\xdb", 100);
    FEED("\xdc\xdb\xdd\xe7\x87\xc0" "ap\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_STRING("heap\n", s_model.ran);
    TEST_ASSERT_EQUAL_UINT(1, s_model.frames);
    TEST_ASSERT_EQUAL_MEMORY(FRAME_BODY, s_model.frame, sizeof FRAME_BODY - 1);
}

TEST_CASE("SLIP escape without a frame delimiter stays text", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("a\xdb\xdd\n", 100);
    drain(QUEUE_MAX);
    EXPECT_QUEUED("a\xdb\xdd\n");
    TEST_ASSERT_EQUAL_UINT(0, s_model.frames);
}

TEST_CASE("a frame boundary follows earlier text and precedes later text for every USB read size", "[td_rx_sync]")
{
    char input[340];
    memset(input, 'x', 300);
    const char suffix[] = "\n" FRAME "version\n";
    memcpy(input + 300, suffix, sizeof suffix - 1);
    const size_t len = 300 + sizeof suffix - 1;
    for (size_t width = 1; width <= 64; width++) {
        model_reset(QUEUE_MAX);
        for (size_t offset = 0; offset < len; offset += width) {
            size_t n = len - offset < width ? len - offset : width;
            feed(input + offset, n);
        }
        TEST_ASSERT_EQUAL_UINT(1, s_model.frames);
        TEST_ASSERT_EQUAL_UINT(301, s_model.frame_after_text);
        TEST_ASSERT_EQUAL_UINT(309, s_model.queued_len);
        TEST_ASSERT_EQUAL_MEMORY("\nversion\n", s_model.queued + 300, 9);
        TEST_ASSERT_EQUAL_UINT(0, s_model.dropped);
    }
}

TEST_CASE("frames still arrive while text overflow discards a line", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("he", 100);
    FEED("ap", 2);
    FEED(FRAME "\nversion\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_STRING("version\n", s_model.ran);
    TEST_ASSERT_EQUAL_UINT(1, s_model.frames);
    TEST_ASSERT_EQUAL_UINT(5, s_model.dropped);
}

TEST_CASE("a frame with a bad CRC counts one error and the text after it runs at once", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED(BAD_CRC_FRAME "version\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_UINT(0, s_model.frames);
    TEST_ASSERT_EQUAL_UINT(1, s_model.bad_frames);
    TEST_ASSERT_EQUAL_STRING("version\n", s_model.ran);
}

TEST_CASE("a truncated frame resynchronises on the next frame without leaking it as text", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("\xc0\x01\x01\x05\x00\x10\x00{\"m\":", 100);
    FEED("version\n", 100);
    FEED(FRAME "heap\n", 100);
    FEED(FRAME FRAME "help\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_UINT(1, s_model.bad_frames);
    TEST_ASSERT_EQUAL_UINT(3, s_model.frames);
    TEST_ASSERT_EQUAL_MEMORY(FRAME_BODY, s_model.frame, sizeof FRAME_BODY - 1);
    TEST_ASSERT_EQUAL_STRING("heap\nhelp\n", s_model.ran);
}

TEST_CASE("a doubled opening delimiter still frames", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("\xc0\xc0\xc0" FRAME "heap\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_UINT(1, s_model.frames);
    TEST_ASSERT_EQUAL_UINT(0, s_model.bad_frames);
    TEST_ASSERT_EQUAL_STRING("heap\n", s_model.ran);
}

TEST_CASE("oversized and malformed frames resynchronise on the next frame", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("\xc0\x01", 100);
    for (size_t i = 0; i < TD_RX_FRAME_MAX; i++) {
        FEED("x", 100);
    }
    FEED("\xc0" FRAME "version\n\xc0\x01\xdb\xc0" FRAME "heap\n", 100);
    FEED("\xc0\x01\xdbx\xc0" FRAME "help\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_UINT(3, s_model.bad_frames);
    TEST_ASSERT_EQUAL_UINT(3, s_model.frames);
    TEST_ASSERT_EQUAL_UINT(0, s_model.dropped);
    TEST_ASSERT_EQUAL_STRING("version\nheap\nhelp\n", s_model.ran);
}

TEST_CASE("a delimiter followed by anything but ver 1 opens no frame", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("\xc0version\n\xc0\xdb\xdd\n\xc0\xc0heap\n", 100);
    drain(QUEUE_MAX);
    EXPECT_QUEUED("version\n\xdb\xdd\nheap\n");
    TEST_ASSERT_EQUAL_UINT(0, s_model.frames);
    TEST_ASSERT_EQUAL_UINT(0, s_model.bad_frames);
}

TEST_CASE("a delimiter after a frame swallows none of the text that follows", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED(FRAME "\xc0" "version\r\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_UINT(1, s_model.frames);
    TEST_ASSERT_EQUAL_UINT(0, s_model.bad_frames);
    TEST_ASSERT_EQUAL_STRING("version\n", s_model.ran);
}

TEST_CASE("text right after a short frame runs at once", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("\xc0\x01\x01\x01\xc0", 100);
    for (uint32_t i = 1; i <= 10; i++) {
        s_model.now = 100 * i;
        FEED("version\r\n", 100);
    }
    FEED("uptime\r\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_UINT(1, s_model.bad_frames);
    TEST_ASSERT_EQUAL_UINT(11, lines_ran());
}

TEST_CASE("text that pads a cut frame to its length never hides the next frame", "[td_rx_sync]")
{
    uint8_t a[64];
    uint8_t b[64];
    message(9, "{\"m\":\"ping\",\"n\":0}", a, sizeof a);
    size_t bn = message(10, "{\"m\":\"ping\",\"n\":1}", b, sizeof b);
    model_reset(QUEUE_MAX);
    feed_bytes(a, 1 + 17);
    FEED("version\r\n", 100);
    feed_bytes(b, bn);
    FEED("uptime\r\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_UINT(1, s_model.frames);
    TEST_ASSERT_EQUAL_UINT(10, s_model.seq);
    TEST_ASSERT_EQUAL_UINT(1, s_model.bad_frames);
    TEST_ASSERT_EQUAL_STRING("uptime\n", s_model.ran);
}

TEST_CASE("a closed frame followed at once by ver 1 opens the next frame", "[td_rx_sync]")
{
    uint8_t a[64];
    uint8_t b[64];
    size_t an = message(1, "{\"m\":\"hello\"}", a, sizeof a);
    size_t bn = message(2, "{\"m\":\"hello\"}", b, sizeof b);
    model_reset(QUEUE_MAX);
    feed_bytes(a, an);
    feed_bytes(b + 1, bn - 1);
    FEED("uptime\n", 100);
    feed_bytes(a, an - 1);
    feed_bytes(b, bn);
    FEED("help\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_UINT(4, s_model.frames);
    TEST_ASSERT_EQUAL_UINT(2, s_model.seq);
    TEST_ASSERT_EQUAL_UINT(0, s_model.bad_frames);
    TEST_ASSERT_EQUAL_STRING("uptime\nhelp\n", s_model.ran);
}

TEST_CASE("random garbage before a frame never costs that frame or runs its body", "[td_rx_sync]")
{
    static const uint8_t alphabet[] = { 0xc0, 0xdb, 0xdc, 0xdd, 0x00, 0x01, 0x02, 0x08, 0x0a, 0x0d, 0x12, 'a', 0x7f };
    uint8_t v[64];
    size_t vn = message(77, "{\"m\":\"ping\",\"n\":5}", v, sizeof v);
    uint32_t x = 7;
    for (int run = 0; run < 10000; run++) {
        uint8_t garbage[40];
        size_t gn = 1 + rnd(&x) % sizeof garbage;
        for (size_t i = 0; i < gn; i++) {
            garbage[i] = alphabet[rnd(&x) % sizeof alphabet];
        }
        model_reset(QUEUE_MAX);
        feed_bytes(garbage, gn);
        feed_bytes(v, vn);
        FEED("\r\nversion\r\n", 100);
        drain(QUEUE_MAX);
        TEST_ASSERT_EQUAL_UINT(77, s_model.seq);
        TEST_ASSERT_NULL(strstr(s_model.ran, "ping"));
    }
}

TEST_CASE("text every 100 ms after a cut frame runs from the frame's deadline on", "[td_rx_sync]")
{
    uint8_t w[64];
    size_t wn = message(1, "{\"m\":\"hello\"}", w, sizeof w);
    for (uint32_t gap = 100; gap <= 500; gap += 200) {
        model_reset(QUEUE_MAX);
        s_model.now = 1000;
        feed_bytes(w, wn / 2);
        size_t after_deadline = 0;
        for (uint32_t i = 1; i <= 20; i++) {
            s_model.now = 1000 + gap * i;
            FEED("uptime\n", 100);
            after_deadline += gap * i >= TD_RX_FRAME_DEADLINE_MS;
        }
        drain(QUEUE_MAX);
        TEST_ASSERT_EQUAL_UINT(after_deadline, lines_ran());
        TEST_ASSERT_EQUAL_UINT(1, s_model.bad_frames);
    }
}

TEST_CASE("the frame deadline runs from the opener, not from the last byte", "[td_rx_sync]")
{
    uint8_t w[64];
    size_t wn = message(1, "{\"m\":\"hello\"}", w, sizeof w);
    model_reset(QUEUE_MAX);
    feed_bytes(w, 5);
    s_model.now = 300;
    feed_bytes(w + 5, 5);
    TEST_ASSERT_EQUAL_UINT32(1, at(TD_RX_FRAME_DEADLINE_MS - 1));
    feed_bytes(w + 10, wn - 10);
    TEST_ASSERT_EQUAL_UINT(1, s_model.frames);

    model_reset(QUEUE_MAX);
    feed_bytes(w, 5);
    s_model.now = 300;
    feed_bytes(w + 5, 5);
    TEST_ASSERT_EQUAL_UINT32(TD_RX_NO_DEADLINE, at(TD_RX_FRAME_DEADLINE_MS));
    TEST_ASSERT_EQUAL_UINT(1, s_model.bad_frames);
    TEST_ASSERT_EQUAL_UINT(0, s_model.frames);
}

TEST_CASE("an abandoned frame gives the port back to text at its deadline", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    TEST_ASSERT_EQUAL_UINT32(TD_RX_NO_DEADLINE, at(0));
    FEED("\xc0", 100);
    TEST_ASSERT_EQUAL_UINT32(TD_RX_FRAME_DEADLINE_MS, at(0));
    TEST_ASSERT_EQUAL_UINT32(TD_RX_NO_DEADLINE, at(TD_RX_FRAME_DEADLINE_MS));
    TEST_ASSERT_EQUAL_UINT(0, s_model.bad_frames);
    s_model.now = 1000;
    FEED("\xc0\x01\x01\x05", 100);
    TEST_ASSERT_EQUAL_UINT32(1, at(1000 + TD_RX_FRAME_DEADLINE_MS - 1));
    at(1000 + TD_RX_FRAME_DEADLINE_MS);
    FEED("heap\n", 100);
    s_model.now = 2000;
    FEED("\xc0\x01\xdb", 100);
    at(2000 + TD_RX_FRAME_DEADLINE_MS);
    FEED("help\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_UINT(2, s_model.bad_frames);
    TEST_ASSERT_EQUAL_UINT(0, s_model.frames);
    TEST_ASSERT_EQUAL_STRING("heap\nhelp\n", s_model.ran);
}

TEST_CASE("a frame cut by a stall past its deadline is lost and its tail is refused", "[td_rx_sync]")
{
    uint8_t w[64];
    size_t wn = message(4, "{\"m\":\"hello\"}", w, sizeof w);
    TEST_ASSERT_EQUAL_HEX8(0x0c, w[wn - 2]);
    model_reset(QUEUE_MAX);
    feed_bytes(w, wn / 2);
    s_model.now = TD_RX_FRAME_DEADLINE_MS;
    feed_bytes(w + wn / 2, wn - wn / 2);
    FEED("uptime\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_UINT(0, s_model.frames);
    TEST_ASSERT_EQUAL_UINT(1, s_model.bad_frames);
    TEST_ASSERT_EQUAL_STRING("", s_model.ran);
    TEST_ASSERT_EQUAL_UINT(1, s_model.refused);
}

TEST_CASE("a line holding a control byte is refused and the next line runs", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("\x01\x02version\nhe\x1b\bap\na\tb\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_UINT(1, s_model.refused);
    TEST_ASSERT_EQUAL_STRING("heap\na\tb\n", s_model.ran);
}

TEST_CASE("a fully escaped maximum body fits the decoded receive buffer", "[td_rx_sync]")
{
    static uint8_t payload[TD_FRAME_PAYLOAD_MAX];
    static uint8_t wire[TD_FRAME_WIRE_MAX];
    for (size_t i = 0; i < sizeof payload; i++) {
        payload[i] = i % 2 ? 0xdb : 0xc0;
    }
    td_frame_t f = { .type = TD_FRAME_CHUNK, .seq = 0xc0, .len = sizeof payload, .payload = payload };
    size_t n = td_frame_encode(&f, wire, sizeof wire);
    TEST_ASSERT_NOT_EQUAL(0, n);
    model_reset(QUEUE_MAX);
    for (size_t off = 0; off < n; off += 2) {
        feed_bytes(wire + off, n - off < 2 ? n - off : 2);
    }
    FEED("version\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_UINT(1, s_model.frames);
    TEST_ASSERT_EQUAL_UINT(0, s_model.bad_frames);
    TEST_ASSERT_EQUAL_UINT(TD_FRAME_BODY_MAX, s_model.frame_len);
    TEST_ASSERT_EQUAL_HEX8(0xc0, s_model.frame[2]);
    TEST_ASSERT_EQUAL_HEX8(0xc0, s_model.frame[6]);
    TEST_ASSERT_EQUAL_HEX8(0xdb, s_model.frame[6 + TD_FRAME_PAYLOAD_MAX - 1]);
    TEST_ASSERT_EQUAL_STRING("version\n", s_model.ran);
}

static struct {
    td_line_t state;
    char line[8];
    char echo[128];
    size_t echo_len;
} s_line;

static td_line_status_t line_byte(int c, bool dropped)
{
    char echo[3];
    size_t len;
    td_line_status_t result = td_line_feed(&s_line.state, c, dropped,
                                          s_line.line, sizeof s_line.line, echo, &len);
    TEST_ASSERT_TRUE(s_line.echo_len + len < sizeof s_line.echo);
    memcpy(s_line.echo + s_line.echo_len, echo, len);
    s_line.echo_len += len;
    return result;
}

TEST_CASE("line editing echoes typed bytes deletion and CR LF", "[td_rx_sync]")
{
    memset(&s_line, 0, sizeof s_line);
    const char input[] = "\baX\bY\x7fz\r";
    for (size_t i = 0; i < sizeof input - 2; i++) {
        TEST_ASSERT_EQUAL(TD_LINE_MORE, line_byte(input[i], false));
    }
    TEST_ASSERT_EQUAL(TD_LINE_OK, line_byte('\r', false));
    TEST_ASSERT_EQUAL_STRING("az", s_line.line);
    TEST_ASSERT_EQUAL_STRING("aX\b \bY\b \bz\r\n", s_line.echo);
    TEST_ASSERT_EQUAL(TD_LINE_OK, line_byte('\n', false));
    TEST_ASSERT_EQUAL_STRING("", s_line.line);
}

TEST_CASE("long lines reject backspace recovery and reset for the next line", "[td_rx_sync]")
{
    memset(&s_line, 0, sizeof s_line);
    const char input[] = "12345678\b9";
    for (size_t i = 0; i < sizeof input - 1; i++) {
        TEST_ASSERT_EQUAL(TD_LINE_MORE, line_byte(input[i], false));
    }
    TEST_ASSERT_EQUAL(TD_LINE_TOO_LONG, line_byte('\n', false));
    TEST_ASSERT_EQUAL_STRING("1234567", s_line.line);
    TEST_ASSERT_EQUAL_STRING("1234567\r\n", s_line.echo);
    line_byte('x', false);
    TEST_ASSERT_EQUAL(TD_LINE_OK, line_byte('\n', false));
    TEST_ASSERT_EQUAL_STRING("x", s_line.line);
}

TEST_CASE("abandon and idle overflow restart without executing a partial line", "[td_rx_sync]")
{
    memset(&s_line, 0, sizeof s_line);
    TEST_ASSERT_EQUAL(TD_LINE_MORE, line_byte(-1, false));
    TEST_ASSERT_EQUAL(TD_LINE_RESTART, line_byte(-1, true));
    line_byte('h', false);
    TEST_ASSERT_EQUAL(TD_LINE_MORE, line_byte(-1, true));
    TEST_ASSERT_EQUAL(TD_LINE_RESTART, line_byte(TD_RX_ABANDON, false));
    TEST_ASSERT_EQUAL_STRING("", s_line.line);
    line_byte('v', false);
    TEST_ASSERT_EQUAL(TD_LINE_OK, line_byte('\n', false));
    TEST_ASSERT_EQUAL_STRING("v", s_line.line);
    TEST_ASSERT_EQUAL_STRING("\r\nh\r\nv\r\n", s_line.echo);
}
