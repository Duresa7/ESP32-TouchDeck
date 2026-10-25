#include "keys.h"
#include "key_sink_log.h"
#include <errno.h>
#include <stdlib.h>
#include <string.h>
#include "esp_timer.h"
#include "td_console.h"

/* The fuzz loop never yields on the console task, so the cap keeps the longest run well under the 5 s idle task
 * watchdog even at the plan's 20 us per step. */
#define FUZZ_MAX_EVENTS 100000
#define FUZZ_MAX_STEP_MS 650
/* The fuzz clock starts this close to its wrap at most, so a run of a few thousand steps can cross it. */
#define FUZZ_CLOCK_HEADROOM_MS 2000000
#define RECORD_LINE_PREFIX "@tdr "

static key_sink_log_t *s_log;

static void send(td_reply_t *r)
{
    td_reply_end(r);
    td_console_write_line(r->buf);
}

static void send_error(const char *message)
{
    char buf[TD_REPLY_MAX];
    td_reply_error(buf, sizeof buf, "keys", message);
    td_console_write_line(buf);
}

static void begin(td_reply_t *r, char *buf, size_t cap, const char *op, bool ok)
{
    td_reply_begin(r, buf, cap, "keys", ok);
    td_reply_str(r, "op", op);
}

static bool require_engine(void)
{
    if (keys_started()) {
        return true;
    }
    send_error("keys not ready");
    return false;
}

/* Digits only, so a sign, a space or a suffix is refused, and so is a number strtoul had to saturate. */
static bool parse_uint(const char *word, uint32_t min, uint32_t max, uint32_t *out)
{
    if (word[0] < '0' || word[0] > '9') {
        return false;
    }
    char *end = NULL;
    errno = 0;
    const unsigned long value = strtoul(word, &end, 10);
    if (errno != 0 || *end != '\0' || value < min || value > max) {
        return false;
    }
    *out = (uint32_t)value;
    return true;
}

/* A decimal usage from 1 to max. With f_keys, F1 to F24 give the keyboard usage of that F-key. */
static bool parse_usage(const char *word, uint32_t max, bool f_keys, uint32_t *out)
{
    if (f_keys && (word[0] == 'F' || word[0] == 'f')) {
        uint32_t n;
        if (!parse_uint(word + 1, 1, 24, &n)) {
            return false;
        }
        *out = key_usage_f(n);
        return true;
    }
    return parse_uint(word, 1, max, out);
}

static bool parse_mod(const char *word, key_mod_t *out)
{
    for (unsigned i = 0; i < KEY_MOD_COUNT; i++) {
        if (strcmp(word, key_mod_name((key_mod_t)i)) == 0) {
            *out = (key_mod_t)i;
            return true;
        }
    }
    return false;
}

static void reply_sent(const char *op, uint32_t usage, keys_result_t res)
{
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    begin(&r, buf, sizeof buf, op, true);
    td_reply_int(&r, "usage", usage);
    td_reply_int(&r, "reports", res.reports);
    td_reply_int(&r, "t", res.t);
    td_reply_int(&r, "sink_us", res.sink_us);
    send(&r);
}

static void cmd_tap(int argc, char **argv)
{
    uint32_t usage;
    if (argc != 3 || !parse_usage(argv[2], 255, true, &usage)) {
        send_error("usage: keys tap <usage 1-255 | F1-F24>");
        return;
    }
    if (require_engine()) {
        reply_sent("tap", usage, keys_tap((uint8_t)usage));
    }
}

static void cmd_consumer(int argc, char **argv)
{
    uint32_t usage;
    if (argc != 3 || !parse_usage(argv[2], 65535, false, &usage)) {
        send_error("usage: keys consumer <usage 1-65535>");
        return;
    }
    if (require_engine()) {
        reply_sent("consumer", usage, keys_consumer((uint16_t)usage));
    }
}

static void cmd_mod(int argc, char **argv)
{
    key_mod_t mod;
    if (argc != 3 || !parse_mod(argv[2], &mod)) {
        send_error("usage: keys mod ctrl|shift|alt|win");
        return;
    }
    if (!require_engine()) {
        return;
    }
    const keys_result_t res = keys_mod(mod);
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    begin(&r, buf, sizeof buf, "mod", true);
    td_reply_str(&r, "mod", key_mod_name(mod));
    td_reply_str(&r, "state", key_mod_state_name(res.mod_state));
    td_reply_int(&r, "t", res.t);
    send(&r);
}

static void cmd_clear(int argc, char **argv)
{
    const bool all = argc == 3 && strcmp(argv[2], "all") == 0;
    if (argc != 2 && !all) {
        send_error("usage: keys clear [all]");
        return;
    }
    if (!require_engine()) {
        return;
    }
    if (all) {
        keys_clear_all();
    } else {
        keys_clear_one_shot();
    }
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    begin(&r, buf, sizeof buf, "clear", true);
    td_reply_bool(&r, "all", all);
    send(&r);
}

static void cmd_state(int argc)
{
    if (argc != 2) {
        send_error("usage: keys state");
        return;
    }
    if (!require_engine()) {
        return;
    }
    const key_engine_state_t st = keys_get_state();
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    begin(&r, buf, sizeof buf, "state", true);
    for (unsigned i = 0; i < KEY_MOD_COUNT; i++) {
        td_reply_str(&r, key_mod_name((key_mod_t)i), key_mod_state_name(st.mods[i].state));
    }
    td_reply_int(&r, "seq", key_sink_log_seq(s_log));
    td_reply_bool(&r, "log", key_sink_log_log_on(s_log));
    td_reply_bool(&r, "recording", key_sink_log_recording(s_log));
    send(&r);
}

static void cmd_log(int argc, char **argv)
{
    const bool on = argc == 3 && strcmp(argv[2], "on") == 0;
    if (!on && !(argc == 3 && strcmp(argv[2], "off") == 0)) {
        send_error("usage: keys log on|off");
        return;
    }
    key_sink_log_set_log(s_log, on);
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    begin(&r, buf, sizeof buf, "log", true);
    td_reply_bool(&r, "on", on);
    send(&r);
}

static void reply_record(void)
{
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    begin(&r, buf, sizeof buf, "record", true);
    td_reply_bool(&r, "recording", key_sink_log_recording(s_log));
    td_reply_int(&r, "count", (long long)key_sink_log_count(s_log));
    td_reply_int(&r, "dropped", key_sink_log_dropped(s_log));
    send(&r);
}

/* Streams the record like screenshot does, because one reply holds at most TD_REPLY_MAX bytes. The count is read
 * once, and the entries below it stay put until the next record start, which runs on this task. */
static void dump_record(void)
{
    const size_t count = key_sink_log_count(s_log);
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    begin(&r, buf, sizeof buf, "record", true);
    td_reply_str(&r, "phase", "start");
    td_reply_int(&r, "count", (long long)count);
    td_reply_int(&r, "dropped", key_sink_log_dropped(s_log));
    send(&r);

    bool ok = true;
    char line[sizeof RECORD_LINE_PREFIX - 1 + KEY_LOG_LINE_MAX];
    memcpy(line, RECORD_LINE_PREFIX, sizeof RECORD_LINE_PREFIX - 1);
    for (size_t i = 0; i < count && ok; i++) {
        key_sink_log_format(key_sink_log_entry(s_log, i), line + sizeof RECORD_LINE_PREFIX - 1, KEY_LOG_LINE_MAX);
        ok = td_console_write_line(line);
    }

    begin(&r, buf, sizeof buf, "record", ok);
    td_reply_str(&r, "phase", "end");
    td_reply_int(&r, "count", (long long)count);
    if (!ok) {
        td_reply_str(&r, "error", "stream aborted");
    }
    send(&r);
}

static void cmd_record(int argc, char **argv)
{
    const char *sub = argc == 3 ? argv[2] : "";
    if (strcmp(sub, "start") == 0) {
        key_sink_log_record_start(s_log);
        reply_record();
    } else if (strcmp(sub, "stop") == 0) {
        key_sink_log_record_stop(s_log);
        reply_record();
    } else if (strcmp(sub, "dump") == 0) {
        dump_record();
    } else {
        send_error("usage: keys record start|stop|dump");
    }
}

typedef struct {
    uint32_t rng;
    uint32_t reports;
    uint32_t violations;
    bool kbd_down;
    bool con_down;
} fuzz_t;

static uint32_t fuzz_next(fuzz_t *f)
{
    f->rng ^= f->rng << 13;
    f->rng ^= f->rng >> 17;
    f->rng ^= f->rng << 5;
    return f->rng;
}

/* The checking sink. A report with a modifier and no key is a violation on its own. A down report leaves its own
 * type down until the release of that type, so a type still down when the step's reports end left a key down. */
static void fuzz_take(void *ctx, const key_report_t *report)
{
    fuzz_t *f = ctx;
    f->reports++;
    if (report->type == KEY_REPORT_KBD && report->kbd.mods != 0 && !key_report_has_key(report)) {
        f->violations++;
    }
    bool *down = report->type == KEY_REPORT_KBD ? &f->kbd_down : &f->con_down;
    *down = key_report_has_key(report);
}

static key_event_t fuzz_event(fuzz_t *f)
{
    const uint32_t r = fuzz_next(f);
    key_event_t ev = { 0 };
    switch (r % 16) {
    case 0: case 1: case 2: case 3: case 4: case 5:
        ev.type = KEY_EV_KEY_TAP;
        ev.usage = (uint16_t)(1 + (r >> 4) % 255);
        break;
    case 6: case 7: case 8: case 9:
        ev.type = KEY_EV_MOD_TAP;
        ev.mod = (key_mod_t)((r >> 4) % KEY_MOD_COUNT);
        break;
    case 10: case 11:
        ev.type = KEY_EV_CONSUMER_TAP;
        ev.usage = (uint16_t)(1 + (r >> 4) % 65535);
        break;
    case 12:
        ev.type = KEY_EV_CLEAR_ONE_SHOT;
        break;
    case 13:
        ev.type = KEY_EV_CLEAR_MODS;
        break;
    case 14:
        ev.type = KEY_EV_RELEASE_ALL;
        break;
    default:
        /* The guarded inputs: a usage of 0, a keyboard usage above 255, a modifier out of range. */
        switch ((r >> 4) % 4) {
        case 0:
            ev.type = KEY_EV_KEY_TAP;
            ev.usage = 0;
            break;
        case 1:
            ev.type = KEY_EV_KEY_TAP;
            ev.usage = (uint16_t)(256 + (r >> 6) % 1000);
            break;
        case 2:
            ev.type = KEY_EV_CONSUMER_TAP;
            ev.usage = 0;
            break;
        default:
            ev.type = KEY_EV_MOD_TAP;
            ev.mod = KEY_MOD_COUNT;
            break;
        }
        break;
    }
    return ev;
}

/* Random events through the pure reducer, on a state of its own, into the checking sink. The engine on the
 * board, the key log and the record see nothing. Every eighth step lands on the double-tap boundary exactly, and
 * the clock starts near its wrap, so a run crosses it. */
static void cmd_fuzz(int argc, char **argv)
{
    uint32_t events, seed;
    if (argc != 4 || !parse_uint(argv[2], 1, FUZZ_MAX_EVENTS, &events) || !parse_uint(argv[3], 0, UINT32_MAX, &seed)) {
        send_error("usage: keys fuzz <events 1-100000> <seed>");
        return;
    }
    fuzz_t f = { .rng = seed ? seed : 0x9E3779B9u };
    const key_sink_t sink = { .report = fuzz_take, .ctx = &f };
    key_engine_state_t st = KEY_ENGINE_STATE_INIT;
    key_engine_out_t out;
    uint32_t now = UINT32_MAX - fuzz_next(&f) % FUZZ_CLOCK_HEADROOM_MS;

    const int64_t start = esp_timer_get_time();
    for (uint32_t i = 0; i < events; i++) {
        const key_event_t ev = fuzz_event(&f);
        const uint32_t r = fuzz_next(&f);
        now += r % 8 == 0 ? KEY_ENGINE_DOUBLE_TAP_MS : (r >> 3) % FUZZ_MAX_STEP_MS;
        st = key_engine_step(st, ev, now, &out);
        for (unsigned k = 0; k < out.count; k++) {
            sink.report(sink.ctx, &out.reports[k]);
        }
        if (f.kbd_down || f.con_down) {
            f.violations++;
            f.kbd_down = f.con_down = false;
        }
    }
    const int64_t elapsed = esp_timer_get_time() - start;

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    begin(&r, buf, sizeof buf, "fuzz", f.violations == 0);
    td_reply_int(&r, "events", events);
    td_reply_int(&r, "seed", seed);
    td_reply_int(&r, "reports", f.reports);
    td_reply_int(&r, "violations", f.violations);
    td_reply_int(&r, "elapsed_us", elapsed);
    if (f.violations != 0) {
        td_reply_str(&r, "error", "violations");
    }
    send(&r);
}

static int cmd_keys(int argc, char **argv)
{
    const char *sub = argc > 1 ? argv[1] : "";
    if (strcmp(sub, "tap") == 0) {
        cmd_tap(argc, argv);
    } else if (strcmp(sub, "consumer") == 0) {
        cmd_consumer(argc, argv);
    } else if (strcmp(sub, "mod") == 0) {
        cmd_mod(argc, argv);
    } else if (strcmp(sub, "clear") == 0) {
        cmd_clear(argc, argv);
    } else if (strcmp(sub, "state") == 0) {
        cmd_state(argc);
    } else if (strcmp(sub, "log") == 0) {
        cmd_log(argc, argv);
    } else if (strcmp(sub, "record") == 0) {
        cmd_record(argc, argv);
    } else if (strcmp(sub, "fuzz") == 0) {
        cmd_fuzz(argc, argv);
    } else {
        send_error("usage: keys tap <usage> | consumer <usage> | mod ctrl|shift|alt|win | clear [all] | state | "
                   "log on|off | record start|stop|dump | fuzz <events> <seed>");
    }
    return 0;
}

const esp_console_cmd_t *keys_console_commands(key_sink_log_t *log, size_t *count)
{
    static const esp_console_cmd_t cmds[] = {
        { .command = "keys", .help = "drive the key engine, read its state, log and record its reports, fuzz the reducer",
          .hint = "tap <usage> | consumer <usage> | mod <m> | clear [all] | state | log on|off | record start|stop|dump | fuzz <n> <seed>",
          .func = cmd_keys },
    };
    s_log = log;
    *count = sizeof cmds / sizeof cmds[0];
    return cmds;
}
