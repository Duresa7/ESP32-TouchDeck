#include <stddef.h>
#include <string.h>
#include "unity.h"
#include "board_pins.h"
#include "ch422g.h"

#define MAX_EVENTS 16

#define EV_WRITE(addr, value) (0x01000000u | ((uint32_t)(addr) << 8) | (value))
#define EV_DELAY(ms) (0x02000000u | (ms))
#define EV_TOUCH_INT 0x03000000u

#define ASSERT_ERR(want, got) TEST_ASSERT_EQUAL_INT((want), (got))

#define EXPECT_EVENTS(...)                                                     \
    do {                                                                       \
        const uint32_t want_[] = {__VA_ARGS__};                                \
        expect_events(want_, sizeof want_ / sizeof want_[0]);                  \
    } while (0)

typedef struct {
    uint32_t events[MAX_EVENTS];
    size_t count;
    bool overflow;
    unsigned writes;
    unsigned nack_at;
    bool touch_int_fails;
} recorder_t;

/* Static so the driver never holds a pointer into a returned test's stack frame. */
static recorder_t s_rec;

/* Called from inside the driver, which holds its lock, so a failed assertion here would leave it locked.
 * Overflow is therefore a flag that expect_events asserts on. */
static void record(recorder_t *r, uint32_t event)
{
    if (r->count == MAX_EVENTS) {
        r->overflow = true;
        return;
    }
    r->events[r->count++] = event;
}

static esp_err_t rec_write(void *ctx, uint8_t addr, uint8_t value)
{
    recorder_t *r = ctx;
    r->writes++;
    record(r, EV_WRITE(addr, value));
    return r->writes == r->nack_at ? ESP_ERR_INVALID_RESPONSE : ESP_OK;
}

static void rec_delay_ms(void *ctx, uint32_t ms)
{
    record(ctx, EV_DELAY(ms));
}

static esp_err_t rec_hold_touch_int_low(void *ctx)
{
    recorder_t *r = ctx;
    record(r, EV_TOUCH_INT);
    return r->touch_int_fails ? ESP_FAIL : ESP_OK;
}

static ch422g_platform_t recorder_platform(void)
{
    return (ch422g_platform_t){
        .write = rec_write,
        .delay_ms = rec_delay_ms,
        .hold_touch_int_low = rec_hold_touch_int_low,
        .ctx = &s_rec,
    };
}

/* Clears the recorder, NACKs the nack_at-th write of this start (0 for none), then starts. */
static esp_err_t start_recorded(unsigned nack_at)
{
    memset(&s_rec, 0, sizeof s_rec);
    s_rec.nack_at = nack_at;
    const ch422g_platform_t platform = recorder_platform();
    return ch422g_start(&platform);
}

static void start_ok(void)
{
    ASSERT_ERR(ESP_OK, start_recorded(0));
    TEST_ASSERT_EQUAL_HEX8(0x1E, ch422g_output());
    s_rec.count = 0;
}

static void nack_next_write(void)
{
    s_rec.nack_at = s_rec.writes + 1;
}

static void expect_events(const uint32_t *want, size_t count)
{
    TEST_ASSERT_FALSE_MESSAGE(s_rec.overflow, "recorder overflow");
    TEST_ASSERT_EQUAL_UINT32(count, s_rec.count);
    TEST_ASSERT_EQUAL_HEX32_ARRAY(want, s_rec.events, count);
}

static void expect_setters_refused(void)
{
    ASSERT_ERR(ESP_ERR_INVALID_STATE, ch422g_set_backlight(false));
    ASSERT_ERR(ESP_ERR_INVALID_STATE, ch422g_lcd_reset(true));
    ASSERT_ERR(ESP_ERR_INVALID_STATE, ch422g_touch_reset(true));
}

TEST_CASE("start replays the waveshare power-up sequence in order", "[ch422g]")
{
    ASSERT_ERR(ESP_OK, start_recorded(0));
    EXPECT_EVENTS(EV_WRITE(0x24, 0x01), EV_WRITE(0x38, 0x2C), EV_DELAY(100), EV_TOUCH_INT,
                  EV_DELAY(100), EV_WRITE(0x38, 0x2E), EV_DELAY(200), EV_WRITE(0x38, 0x1E));
    TEST_ASSERT_EQUAL_HEX8(0x1E, ch422g_output());
}

TEST_CASE("backlight off clears only DISP and on sets it again", "[ch422g]")
{
    start_ok();
    ASSERT_ERR(ESP_OK, ch422g_set_backlight(false));
    TEST_ASSERT_EQUAL_HEX8(0x1A, ch422g_output());
    EXPECT_EVENTS(EV_WRITE(0x38, 0x1A));

    ASSERT_ERR(ESP_OK, ch422g_set_backlight(true));
    TEST_ASSERT_EQUAL_HEX8(0x1E, ch422g_output());
    EXPECT_EVENTS(EV_WRITE(0x38, 0x1A), EV_WRITE(0x38, 0x1E));
}

TEST_CASE("reset lines are active low", "[ch422g]")
{
    start_ok();
    ASSERT_ERR(ESP_OK, ch422g_touch_reset(true));
    TEST_ASSERT_EQUAL_HEX8(0x1C, ch422g_output());
    ASSERT_ERR(ESP_OK, ch422g_touch_reset(false));
    TEST_ASSERT_EQUAL_HEX8(0x1E, ch422g_output());

    ASSERT_ERR(ESP_OK, ch422g_lcd_reset(true));
    TEST_ASSERT_EQUAL_HEX8(0x16, ch422g_output());
    ASSERT_ERR(ESP_OK, ch422g_lcd_reset(false));
    TEST_ASSERT_EQUAL_HEX8(0x1E, ch422g_output());

    EXPECT_EVENTS(EV_WRITE(0x38, 0x1C), EV_WRITE(0x38, 0x1E), EV_WRITE(0x38, 0x16), EV_WRITE(0x38, 0x1E));
}

TEST_CASE("setters compose on the cached output", "[ch422g]")
{
    start_ok();
    ASSERT_ERR(ESP_OK, ch422g_set_backlight(false));
    TEST_ASSERT_EQUAL_HEX8(0x1A, ch422g_output());
    ASSERT_ERR(ESP_OK, ch422g_touch_reset(true));
    TEST_ASSERT_EQUAL_HEX8(0x18, ch422g_output());

    ASSERT_ERR(ESP_OK, ch422g_set_backlight(true));
    TEST_ASSERT_EQUAL_HEX8(0x1C, ch422g_output());
    ASSERT_ERR(ESP_OK, ch422g_touch_reset(false));
    TEST_ASSERT_EQUAL_HEX8(0x1E, ch422g_output());

    EXPECT_EVENTS(EV_WRITE(0x38, 0x1A), EV_WRITE(0x38, 0x18), EV_WRITE(0x38, 0x1C), EV_WRITE(0x38, 0x1E));
}

TEST_CASE("a setter that changes nothing still writes the unchanged byte", "[ch422g]")
{
    start_ok();
    ASSERT_ERR(ESP_OK, ch422g_set_backlight(true));
    ASSERT_ERR(ESP_OK, ch422g_lcd_reset(false));
    TEST_ASSERT_EQUAL_HEX8(0x1E, ch422g_output());
    EXPECT_EVENTS(EV_WRITE(0x38, 0x1E), EV_WRITE(0x38, 0x1E));
}

TEST_CASE("a NACKed setter write returns the error and keeps the cached output", "[ch422g]")
{
    start_ok();
    nack_next_write();
    ASSERT_ERR(ESP_ERR_INVALID_RESPONSE, ch422g_set_backlight(false));
    TEST_ASSERT_EQUAL_HEX8(0x1E, ch422g_output());
    EXPECT_EVENTS(EV_WRITE(0x38, 0x1A));

    ASSERT_ERR(ESP_OK, ch422g_touch_reset(true));
    TEST_ASSERT_EQUAL_HEX8(0x1C, ch422g_output());
    EXPECT_EVENTS(EV_WRITE(0x38, 0x1A), EV_WRITE(0x38, 0x1C));
}

TEST_CASE("a start whose first write NACKs fails and leaves the driver stopped", "[ch422g]")
{
    ASSERT_ERR(ESP_ERR_INVALID_RESPONSE, start_recorded(1));
    EXPECT_EVENTS(EV_WRITE(0x24, 0x01));
    TEST_ASSERT_EQUAL_HEX8(0x00, ch422g_output());

    s_rec.count = 0;
    expect_setters_refused();
    TEST_ASSERT_EQUAL_UINT32(0, s_rec.count);

    start_ok();
    ASSERT_ERR(ESP_OK, ch422g_set_backlight(false));
    TEST_ASSERT_EQUAL_HEX8(0x1A, ch422g_output());
}

TEST_CASE("a start that fails midway stops there and stops a running driver", "[ch422g]")
{
    start_ok();
    ASSERT_ERR(ESP_ERR_INVALID_RESPONSE, start_recorded(3));
    EXPECT_EVENTS(EV_WRITE(0x24, 0x01), EV_WRITE(0x38, 0x2C), EV_DELAY(100), EV_TOUCH_INT,
                  EV_DELAY(100), EV_WRITE(0x38, 0x2E));
    TEST_ASSERT_EQUAL_HEX8(0x2C, ch422g_output());

    s_rec.count = 0;
    expect_setters_refused();
    TEST_ASSERT_EQUAL_UINT32(0, s_rec.count);
}

TEST_CASE("a start whose touch interrupt hold fails stops before the release", "[ch422g]")
{
    memset(&s_rec, 0, sizeof s_rec);
    s_rec.touch_int_fails = true;
    const ch422g_platform_t platform = recorder_platform();
    ASSERT_ERR(ESP_FAIL, ch422g_start(&platform));
    EXPECT_EVENTS(EV_WRITE(0x24, 0x01), EV_WRITE(0x38, 0x2C), EV_DELAY(100), EV_TOUCH_INT);
    TEST_ASSERT_EQUAL_HEX8(0x2C, ch422g_output());

    s_rec.count = 0;
    expect_setters_refused();
    TEST_ASSERT_EQUAL_UINT32(0, s_rec.count);
}

TEST_CASE("a NACK on the final start write keeps the last acknowledged byte and stops the driver", "[ch422g]")
{
    ASSERT_ERR(ESP_ERR_INVALID_RESPONSE, start_recorded(4));
    EXPECT_EVENTS(EV_WRITE(0x24, 0x01), EV_WRITE(0x38, 0x2C), EV_DELAY(100), EV_TOUCH_INT,
                  EV_DELAY(100), EV_WRITE(0x38, 0x2E), EV_DELAY(200), EV_WRITE(0x38, 0x1E));
    TEST_ASSERT_EQUAL_HEX8(0x2E, ch422g_output());

    s_rec.count = 0;
    expect_setters_refused();
    TEST_ASSERT_EQUAL_UINT32(0, s_rec.count);
}

TEST_CASE("start rejects a NULL platform or a NULL callback and leaves the driver as it was", "[ch422g]")
{
    start_ok();
    ASSERT_ERR(ESP_ERR_INVALID_ARG, ch422g_start(NULL));

    ch422g_platform_t platform = recorder_platform();
    platform.write = NULL;
    ASSERT_ERR(ESP_ERR_INVALID_ARG, ch422g_start(&platform));
    platform = recorder_platform();
    platform.delay_ms = NULL;
    ASSERT_ERR(ESP_ERR_INVALID_ARG, ch422g_start(&platform));
    platform = recorder_platform();
    platform.hold_touch_int_low = NULL;
    ASSERT_ERR(ESP_ERR_INVALID_ARG, ch422g_start(&platform));
    TEST_ASSERT_EQUAL_UINT32(0, s_rec.count);

    ASSERT_ERR(ESP_OK, ch422g_set_backlight(false));
    TEST_ASSERT_EQUAL_HEX8(0x1A, ch422g_output());
}
