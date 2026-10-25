#include <stdio.h>
#include <string.h>
#include "esp_timer.h"
#include "unity.h"
#include "key_engine.h"

#define F15 106
#define F16 107
#define F17 108

static key_engine_state_t s_state;
static key_engine_out_t s_out;

static key_event_t key_tap(uint16_t usage)
{
    return (key_event_t){ .type = KEY_EV_KEY_TAP, .usage = usage };
}

static key_event_t mod_tap(key_mod_t mod)
{
    return (key_event_t){ .type = KEY_EV_MOD_TAP, .mod = mod };
}

static key_event_t consumer_tap(uint16_t usage)
{
    return (key_event_t){ .type = KEY_EV_CONSUMER_TAP, .usage = usage };
}

static key_event_t plain(key_event_type_t type)
{
    return (key_event_t){ .type = type };
}

/* Every step is checked here: a report with a key is followed in the same step by the release of its own type,
 * and no report carries a modifier without a key. */
static void step(key_event_t ev, uint32_t now_ms)
{
    s_state = key_engine_step(s_state, ev, now_ms, &s_out);
    TEST_ASSERT_TRUE(s_out.count <= KEY_ENGINE_OUT_MAX);
    bool down[2] = { false, false };
    for (unsigned i = 0; i < s_out.count; i++) {
        const key_report_t *r = &s_out.reports[i];
        TEST_ASSERT_FALSE_MESSAGE(r->type == KEY_REPORT_KBD && r->kbd.mods != 0 && !key_report_has_key(r),
                                  "a report carried a modifier and no key");
        down[r->type == KEY_REPORT_CON] = key_report_has_key(r);
    }
    TEST_ASSERT_FALSE_MESSAGE(down[0], "the step left a keyboard key down");
    TEST_ASSERT_FALSE_MESSAGE(down[1], "the step left a consumer usage down");
}

static void expect_kbd(unsigned i, uint8_t mods, uint8_t usage)
{
    TEST_ASSERT_TRUE(i < s_out.count);
    const key_report_t *r = &s_out.reports[i];
    TEST_ASSERT_EQUAL_INT(KEY_REPORT_KBD, r->type);
    TEST_ASSERT_EQUAL_UINT8(mods, r->kbd.mods);
    TEST_ASSERT_EQUAL_UINT8(0, r->kbd.reserved);
    TEST_ASSERT_EQUAL_UINT8(usage, r->kbd.keys[0]);
    for (unsigned k = 1; k < KEY_REPORT_KEYS; k++) {
        TEST_ASSERT_EQUAL_UINT8(0, r->kbd.keys[k]);
    }
}

static void expect_con(unsigned i, uint16_t usage)
{
    TEST_ASSERT_TRUE(i < s_out.count);
    TEST_ASSERT_EQUAL_INT(KEY_REPORT_CON, s_out.reports[i].type);
    TEST_ASSERT_EQUAL_UINT16(usage, s_out.reports[i].con.usage);
}

static void expect_key_pair(uint8_t mods, uint8_t usage)
{
    TEST_ASSERT_EQUAL_UINT8(2, s_out.count);
    expect_kbd(0, mods, usage);
    expect_kbd(1, 0, 0);
}

static void expect_mod(key_mod_t mod, key_mod_state_t state)
{
    TEST_ASSERT_EQUAL_STRING(key_mod_state_name(state), key_mod_state_name(s_state.mods[mod].state));
}

static void expect_all_off(void)
{
    for (unsigned i = 0; i < KEY_MOD_COUNT; i++) {
        expect_mod((key_mod_t)i, KEY_MOD_OFF);
    }
}

static void reset(void)
{
    s_state = KEY_ENGINE_STATE_INIT;
    memset(&s_out, 0, sizeof s_out);
}

typedef struct {
    const char *name;
    key_mod_state_t before;
    uint32_t tapped_ms;
    uint32_t now_ms;
    key_mod_state_t after;
} mod_case_t;

TEST_CASE("a modifier tap follows the OFF, ONE_SHOT, LOCKED table", "[key_engine]")
{
    static const mod_case_t cases[] = {
        { "OFF to ONE_SHOT", KEY_MOD_OFF, 0, 1000, KEY_MOD_ONE_SHOT },
        { "ONE_SHOT tapped again at 299 ms locks", KEY_MOD_ONE_SHOT, 1000, 1299, KEY_MOD_LOCKED },
        { "ONE_SHOT tapped again at 300 ms locks", KEY_MOD_ONE_SHOT, 1000, 1300, KEY_MOD_LOCKED },
        { "ONE_SHOT tapped again at 301 ms turns off", KEY_MOD_ONE_SHOT, 1000, 1301, KEY_MOD_OFF },
        { "ONE_SHOT tapped again at 0 ms locks", KEY_MOD_ONE_SHOT, 1000, 1000, KEY_MOD_LOCKED },
        { "ONE_SHOT tapped across a clock wrap at 200 ms locks", KEY_MOD_ONE_SHOT, 0xFFFFFF9Cu, 100, KEY_MOD_LOCKED },
        { "LOCKED to OFF", KEY_MOD_LOCKED, 1000, 1001, KEY_MOD_OFF },
        { "LOCKED to OFF much later", KEY_MOD_LOCKED, 1000, 99999, KEY_MOD_OFF },
    };
    /* The other three modifiers hold a different state each and must come out as they went in. */
    static const key_mod_state_t others[] = { KEY_MOD_LOCKED, KEY_MOD_ONE_SHOT, KEY_MOD_OFF };
    for (size_t i = 0; i < sizeof cases / sizeof cases[0]; i++) {
        for (unsigned m = 0; m < KEY_MOD_COUNT; m++) {
            reset();
            unsigned k = 0;
            for (unsigned other = 0; other < KEY_MOD_COUNT; other++) {
                if (other != m) {
                    s_state.mods[other] = (key_mod_slot_t){ .state = others[k++], .tapped_ms = 0 };
                }
            }
            s_state.mods[m] = (key_mod_slot_t){ .state = cases[i].before, .tapped_ms = cases[i].tapped_ms };
            step(mod_tap((key_mod_t)m), cases[i].now_ms);
            TEST_ASSERT_EQUAL_UINT8_MESSAGE(0, s_out.count, cases[i].name);
            TEST_ASSERT_EQUAL_STRING_MESSAGE(key_mod_state_name(cases[i].after),
                                             key_mod_state_name(s_state.mods[m].state), cases[i].name);
            k = 0;
            for (unsigned other = 0; other < KEY_MOD_COUNT; other++) {
                if (other != m) {
                    expect_mod((key_mod_t)other, others[k++]);
                }
            }
        }
    }
}

TEST_CASE("a key tap with no modifier emits the key and its release", "[key_engine]")
{
    reset();
    step(key_tap(F15), 0);
    expect_key_pair(0, F15);
    expect_all_off();
}

TEST_CASE("a one-shot modifier rides on the next key and then clears", "[key_engine]")
{
    reset();
    step(mod_tap(KEY_MOD_SHIFT), 100);
    TEST_ASSERT_EQUAL_UINT8(0, s_out.count);
    step(key_tap(F15), 200);
    expect_key_pair(2, F15);
    expect_mod(KEY_MOD_SHIFT, KEY_MOD_OFF);
    step(key_tap(F16), 300);
    expect_key_pair(0, F16);
}

TEST_CASE("a locked modifier rides on every key until it is tapped off", "[key_engine]")
{
    reset();
    step(mod_tap(KEY_MOD_CTRL), 100);
    step(mod_tap(KEY_MOD_CTRL), 250);
    expect_mod(KEY_MOD_CTRL, KEY_MOD_LOCKED);
    step(key_tap(F15), 1000);
    expect_key_pair(1, F15);
    step(key_tap(F16), 5000);
    expect_key_pair(1, F16);
    expect_mod(KEY_MOD_CTRL, KEY_MOD_LOCKED);
    step(mod_tap(KEY_MOD_CTRL), 9000);
    expect_mod(KEY_MOD_CTRL, KEY_MOD_OFF);
    step(key_tap(F15), 9100);
    expect_key_pair(0, F15);
}

TEST_CASE("two one-shot modifiers apply to one key and both clear", "[key_engine]")
{
    reset();
    step(mod_tap(KEY_MOD_SHIFT), 100);
    step(mod_tap(KEY_MOD_CTRL), 200);
    step(key_tap(F17), 300);
    expect_key_pair(3, F17);
    expect_all_off();
}

TEST_CASE("a one-shot and a locked modifier apply together and only the one-shot clears", "[key_engine]")
{
    reset();
    step(mod_tap(KEY_MOD_WIN), 100);
    step(mod_tap(KEY_MOD_WIN), 200);
    step(mod_tap(KEY_MOD_ALT), 1000);
    step(key_tap(F15), 1100);
    expect_key_pair(8 | 4, F15);
    expect_mod(KEY_MOD_WIN, KEY_MOD_LOCKED);
    expect_mod(KEY_MOD_ALT, KEY_MOD_OFF);
}

TEST_CASE("the modifier byte maps ctrl shift alt win to 1 2 4 8", "[key_engine]")
{
    reset();
    static const uint8_t bits[KEY_MOD_COUNT] = { 1, 2, 4, 8 };
    for (unsigned m = 0; m < KEY_MOD_COUNT; m++) {
        reset();
        step(mod_tap((key_mod_t)m), 0);
        TEST_ASSERT_EQUAL_UINT8(bits[m], key_engine_mods_byte(&s_state));
        step(key_tap(F15), 10);
        expect_key_pair(bits[m], F15);
    }
}

TEST_CASE("a consumer tap carries no modifier and leaves a locked one in place", "[key_engine]")
{
    reset();
    step(mod_tap(KEY_MOD_CTRL), 100);
    step(mod_tap(KEY_MOD_CTRL), 200);
    step(mod_tap(KEY_MOD_SHIFT), 300);
    step(consumer_tap(205), 400);
    TEST_ASSERT_EQUAL_UINT8(2, s_out.count);
    expect_con(0, 205);
    expect_con(1, 0);
    expect_mod(KEY_MOD_CTRL, KEY_MOD_LOCKED);
    expect_mod(KEY_MOD_SHIFT, KEY_MOD_ONE_SHOT);
}

TEST_CASE("CLEAR_ONE_SHOT keeps a locked modifier and emits nothing", "[key_engine]")
{
    reset();
    step(mod_tap(KEY_MOD_CTRL), 100);
    step(mod_tap(KEY_MOD_CTRL), 200);
    step(mod_tap(KEY_MOD_SHIFT), 300);
    step(plain(KEY_EV_CLEAR_ONE_SHOT), 400);
    TEST_ASSERT_EQUAL_UINT8(0, s_out.count);
    expect_mod(KEY_MOD_CTRL, KEY_MOD_LOCKED);
    expect_mod(KEY_MOD_SHIFT, KEY_MOD_OFF);
}

TEST_CASE("CLEAR_MODS turns every modifier off and emits nothing", "[key_engine]")
{
    reset();
    step(mod_tap(KEY_MOD_CTRL), 100);
    step(mod_tap(KEY_MOD_CTRL), 200);
    step(mod_tap(KEY_MOD_SHIFT), 300);
    step(mod_tap(KEY_MOD_WIN), 400);
    step(plain(KEY_EV_CLEAR_MODS), 500);
    TEST_ASSERT_EQUAL_UINT8(0, s_out.count);
    expect_all_off();
    step(key_tap(F15), 600);
    expect_key_pair(0, F15);
}

TEST_CASE("RELEASE_ALL emits both release reports and keeps the modifiers", "[key_engine]")
{
    reset();
    step(mod_tap(KEY_MOD_ALT), 100);
    step(mod_tap(KEY_MOD_ALT), 200);
    step(mod_tap(KEY_MOD_SHIFT), 300);
    step(plain(KEY_EV_RELEASE_ALL), 400);
    TEST_ASSERT_EQUAL_UINT8(2, s_out.count);
    expect_kbd(0, 0, 0);
    expect_con(1, 0);
    expect_mod(KEY_MOD_ALT, KEY_MOD_LOCKED);
    expect_mod(KEY_MOD_SHIFT, KEY_MOD_ONE_SHOT);
    step(key_tap(F15), 500);
    expect_key_pair(4 | 2, F15);
}

TEST_CASE("a tap of usage 0 or out of range emits nothing and keeps a one-shot armed", "[key_engine]")
{
    reset();
    step(mod_tap(KEY_MOD_SHIFT), 100);
    step(key_tap(0), 200);
    TEST_ASSERT_EQUAL_UINT8(0, s_out.count);
    step(key_tap(256), 210);
    TEST_ASSERT_EQUAL_UINT8(0, s_out.count);
    step(consumer_tap(0), 220);
    TEST_ASSERT_EQUAL_UINT8(0, s_out.count);
    step((key_event_t){ .type = KEY_EV_MOD_TAP, .mod = KEY_MOD_COUNT }, 230);
    TEST_ASSERT_EQUAL_UINT8(0, s_out.count);
    expect_mod(KEY_MOD_SHIFT, KEY_MOD_ONE_SHOT);
    step(key_tap(F15), 300);
    expect_key_pair(2, F15);
}

TEST_CASE("F1 to F24 map to usages 58 to 69 and 104 to 115", "[key_engine]")
{
    reset();
    TEST_ASSERT_EQUAL_UINT8(0, key_usage_f(0));
    for (unsigned n = 1; n <= 12; n++) {
        TEST_ASSERT_EQUAL_UINT8(58 + n - 1, key_usage_f(n));
    }
    for (unsigned n = 13; n <= 24; n++) {
        TEST_ASSERT_EQUAL_UINT8(104 + n - 13, key_usage_f(n));
    }
    TEST_ASSERT_EQUAL_UINT8(0, key_usage_f(25));
    TEST_ASSERT_EQUAL_UINT8(F15, key_usage_f(15));
}

/* The perf probe of the plan. The mix is one modifier tap per three key taps, with the clock stepping 150 ms, so
 * every transition runs. The number prints for the report, and the plan fails an average above 20 us. */
TEST_CASE("100000 reducer steps average under 20 microseconds", "[key_engine]")
{
    reset();
    const unsigned steps = 100000;
    uint32_t now = 0;
    const int64_t start = esp_timer_get_time();
    for (unsigned i = 0; i < steps; i++) {
        const key_event_t ev = i % 4 == 3 ? mod_tap((key_mod_t)(i % KEY_MOD_COUNT)) : key_tap((uint16_t)(58 + i % 12));
        now += 150;
        s_state = key_engine_step(s_state, ev, now, &s_out);
        if (s_out.count > 0) {
            TEST_ASSERT_TRUE(key_report_is_release(&s_out.reports[s_out.count - 1]));
        }
    }
    const int64_t elapsed = esp_timer_get_time() - start;
    printf("key_engine_step: %u steps in %lld us, %.3f us per step\n", steps, (long long)elapsed,
           (double)elapsed / steps);
    TEST_ASSERT_TRUE_MESSAGE(elapsed < (int64_t)steps * 20, "a reducer step averaged more than 20 us");
}
