#pragma once

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define KEY_REPORT_KEYS 6

typedef enum {
    KEY_REPORT_KBD,
    KEY_REPORT_CON,
} key_report_type_t;

/*
 * One HID report, in the bytes the USB sink will send. kbd is the boot keyboard report: the modifier byte, a
 * reserved byte, and six key slots. con is one 16-bit consumer control usage. A kbd report with mods 0 and no
 * key is the all-released report, and con 0 is the empty consumer report.
 */
typedef struct {
    key_report_type_t type;
    union {
        struct {
            uint8_t mods;
            uint8_t reserved;
            uint8_t keys[KEY_REPORT_KEYS];
        } kbd;
        struct {
            uint16_t usage;
        } con;
    };
} key_report_t;

/* Where reports go. The engine and keys.c never know which sink they have. */
typedef struct {
    void (*report)(void *ctx, const key_report_t *report);
    void *ctx;
} key_sink_t;

static inline key_report_t key_report_kbd(uint8_t mods, uint8_t usage)
{
    key_report_t r = { .type = KEY_REPORT_KBD };
    r.kbd.mods = mods;
    r.kbd.keys[0] = usage;
    return r;
}

static inline key_report_t key_report_con(uint16_t usage)
{
    key_report_t r = { .type = KEY_REPORT_CON };
    r.con.usage = usage;
    return r;
}

static inline bool key_report_has_key(const key_report_t *r)
{
    if (r->type == KEY_REPORT_CON) {
        return r->con.usage != 0;
    }
    for (unsigned i = 0; i < KEY_REPORT_KEYS; i++) {
        if (r->kbd.keys[i] != 0) {
            return true;
        }
    }
    return false;
}

/* The all-released keyboard report or the empty consumer report. */
static inline bool key_report_is_release(const key_report_t *r)
{
    return !key_report_has_key(r) && (r->type == KEY_REPORT_CON || r->kbd.mods == 0);
}

#ifdef __cplusplus
}
#endif
