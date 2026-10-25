#include <stdint.h>
#include <string.h>
#include "unity.h"
#include "td_rx_sync.h"

#define QUEUE_MAX 2048
#define LOG_MAX 8192
#define RAN_MAX 16384

/*
 * Stands in for the console: bytes queued by td_rx_sync go into a ring, and a
 * reader pops them the way read_line does. `ran` collects each line it would
 * run, newline terminated, so a test asserts which commands the board executes.
 */
static struct {
    td_rx_sync_t sync;
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
    size_t line_len;
    char ran[RAN_MAX];
    size_t ran_len;
} s_model;

static void model_reset(size_t cap)
{
    memset(&s_model, 0, sizeof s_model);
    s_model.cap = cap;
    s_model.queued_last_read = true;
}

static size_t feed_with_space(const char *s, size_t n, size_t space)
{
    char buf[64];
    TEST_ASSERT_TRUE(n <= sizeof buf);
    memcpy(buf, s, n);
    size_t dropped_before = s_model.dropped;
    size_t m = td_rx_sync(&s_model.sync, buf, n, space, &s_model.dropped);
    if (s_model.dropped > dropped_before && s_model.queued_last_read) {
        s_model.overflows++;
    }
    s_model.queued_last_read = m > 0;
    TEST_ASSERT_TRUE_MESSAGE(m <= space, "queued more than the free space");
    for (size_t i = 0; i < m; i++) {
        s_model.ring[(s_model.rd + s_model.used) % QUEUE_MAX] = buf[i];
        s_model.used++;
        TEST_ASSERT_TRUE(s_model.queued_len < LOG_MAX);
        s_model.queued[s_model.queued_len++] = buf[i];
    }
    return m;
}

static size_t feed(const char *s, size_t n)
{
    return feed_with_space(s, n, s_model.cap - s_model.used);
}

static void console_byte(char c)
{
    if (c == TD_RX_ABANDON) {
        s_model.line_len = 0;
    } else if (c == '\r' || c == '\n') {
        if (s_model.line_len > 0) {
            TEST_ASSERT_TRUE(s_model.ran_len + s_model.line_len + 1 < RAN_MAX);
            memcpy(s_model.ran + s_model.ran_len, s_model.line, s_model.line_len);
            s_model.ran_len += s_model.line_len;
            s_model.ran[s_model.ran_len++] = '\n';
            s_model.ran[s_model.ran_len] = '\0';
        }
        s_model.line_len = 0;
    } else {
        TEST_ASSERT_TRUE(s_model.line_len < sizeof s_model.line);
        s_model.line[s_model.line_len++] = c;
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
    TEST_ASSERT_EQUAL_UINT(0, s_model.line_len);

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
