#include <stdint.h>
#include <string.h>
#include "unity.h"
#include "td_rx_sync.h"

#define QUEUE_MAX 2048
#define LOG_MAX 8192
#define RAN_MAX 16384

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
    td_line_t line_state;
    uint8_t frame[TD_RX_FRAME_MAX];
    size_t frame_len;
    size_t frames;
    size_t bad_frames;
    char ran[RAN_MAX];
    size_t ran_len;
} s_model;

static void frame_received(const uint8_t *body, size_t len, void *context)
{
    TEST_ASSERT_EQUAL_PTR(&s_model, context);
    if (!body) {
        s_model.bad_frames++;
        return;
    }
    memcpy(s_model.frame, body, len);
    s_model.frame_len = len;
    s_model.frames++;
}

static void model_reset(size_t cap)
{
    memset(&s_model, 0, sizeof s_model);
    s_model.cap = cap;
    s_model.queued_last_read = true;
    s_model.sync.on_frame = frame_received;
    s_model.sync.context = &s_model;
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
    char echo[3];
    size_t echo_len;
    td_line_status_t status = td_line_feed(&s_model.line_state, (unsigned char)c, false,
                                          s_model.line, sizeof s_model.line, echo, &echo_len);
    TEST_ASSERT_NOT_EQUAL(TD_LINE_TOO_LONG, status);
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

TEST_CASE("a frame between text lines reaches only the frame receiver", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("heap\n\xc0\x01\x00\xdb\xdc\xdb\xdd\xc0version\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_STRING("heap\nversion\n", s_model.ran);
    TEST_ASSERT_EQUAL_UINT(1, s_model.frames);
    TEST_ASSERT_EQUAL_UINT(4, s_model.frame_len);
    TEST_ASSERT_EQUAL_MEMORY("\x01\x00\xc0\xdb", s_model.frame, 4);
    TEST_ASSERT_EQUAL_UINT(0, s_model.dropped);
}

TEST_CASE("a frame and its escape survive separate USB reads", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("he\xc0\x01\xdb", 100);
    FEED("\xdd\x00\xc0" "ap\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_STRING("heap\n", s_model.ran);
    TEST_ASSERT_EQUAL_UINT(1, s_model.frames);
    TEST_ASSERT_EQUAL_UINT(3, s_model.frame_len);
    TEST_ASSERT_EQUAL_MEMORY("\x01\xdb\x00", s_model.frame, 3);
}

TEST_CASE("SLIP escape without a frame delimiter stays text", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("a\xdb\xdd\n", 100);
    drain(QUEUE_MAX);
    EXPECT_QUEUED("a\xdb\xdd\n");
    TEST_ASSERT_EQUAL_UINT(0, s_model.frames);
}

TEST_CASE("frames still arrive while text overflow discards a line", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("he", 100);
    FEED("ap", 2);
    FEED("\xc0\x01\x00\xc0\nversion\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_STRING("version\n", s_model.ran);
    TEST_ASSERT_EQUAL_UINT(1, s_model.frames);
    TEST_ASSERT_EQUAL_UINT(5, s_model.dropped);
}

TEST_CASE("oversized and malformed frames drop through their closing delimiter", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("\xc0", 100);
    for (size_t i = 0; i < TD_RX_FRAME_MAX + 1; i++) {
        FEED("x", 100);
    }
    FEED("\xc0version\n\xc0\xdb\xc0heap\n\xc0\xdbx\xc0", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_UINT(3, s_model.bad_frames);
    TEST_ASSERT_EQUAL_UINT(0, s_model.frames);
    TEST_ASSERT_EQUAL_UINT(0, s_model.dropped);
    TEST_ASSERT_EQUAL_STRING("version\nheap\n", s_model.ran);
}

TEST_CASE("a fully escaped maximum body fits the decoded receive buffer", "[td_rx_sync]")
{
    model_reset(QUEUE_MAX);
    FEED("\xc0", 100);
    for (size_t i = 0; i < 1032; i++) {
        FEED("\xdb\xdc", 100);
    }
    FEED("\xc0\xc0x\xc0version\n", 100);
    drain(QUEUE_MAX);
    TEST_ASSERT_EQUAL_UINT(2, s_model.frames);
    TEST_ASSERT_EQUAL_UINT(0, s_model.bad_frames);
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
