#pragma once

#include <stdbool.h>
#include <stdint.h>
#include "key_sink.h"

#ifdef __cplusplus
extern "C" {
#endif

/* A second tap on a one-shot modifier this soon after the first locks it. Later, it turns the modifier off. */
#define KEY_ENGINE_DOUBLE_TAP_MS 300

/* Indexed so that the HID modifier bit of a modifier is 1 << mod: Ctrl 1, Shift 2, Alt 4, Win 8. */
typedef enum {
    KEY_MOD_CTRL,
    KEY_MOD_SHIFT,
    KEY_MOD_ALT,
    KEY_MOD_WIN,
    KEY_MOD_COUNT,
} key_mod_t;

typedef enum {
    KEY_MOD_OFF,
    KEY_MOD_ONE_SHOT,
    KEY_MOD_LOCKED,
} key_mod_state_t;

typedef struct {
    key_mod_state_t state;
    uint32_t tapped_ms;
} key_mod_slot_t;

/* The whole engine state. It holds no key, so nothing is ever down between events. */
typedef struct {
    key_mod_slot_t mods[KEY_MOD_COUNT];
} key_engine_state_t;

#define KEY_ENGINE_STATE_INIT ((key_engine_state_t){ 0 })

typedef enum {
    KEY_EV_KEY_TAP,        /* usage: a keyboard usage, 1 to 255 */
    KEY_EV_MOD_TAP,        /* mod */
    KEY_EV_CONSUMER_TAP,   /* usage: a consumer usage, 1 to 65535 */
    KEY_EV_CLEAR_ONE_SHOT,
    KEY_EV_CLEAR_MODS,
    KEY_EV_RELEASE_ALL,
} key_event_type_t;

typedef struct {
    key_event_type_t type;
    uint16_t usage;
    key_mod_t mod;
} key_event_t;

#define KEY_ENGINE_OUT_MAX 2

typedef struct {
    key_report_t reports[KEY_ENGINE_OUT_MAX];
    uint8_t count;
} key_engine_out_t;

/*
 * The pure reducer. It returns the state after the event and leaves the reports to send, in order, in *out.
 *
 * KEY_TAP emits the key with every active modifier, then the all-released report, and returns one-shot
 * modifiers to OFF. MOD_TAP moves OFF to ONE_SHOT, a second tap within KEY_ENGINE_DOUBLE_TAP_MS to LOCKED, and a
 * tap on ONE_SHOT after that or on LOCKED to OFF. CONSUMER_TAP emits the usage, then the empty consumer report,
 * and leaves every modifier as it is. CLEAR_ONE_SHOT returns ONE_SHOT modifiers to OFF and keeps LOCKED ones.
 * CLEAR_MODS returns every modifier to OFF. RELEASE_ALL emits the all-released report and then the empty
 * consumer report, for a sink that wants a clean slate. A tap with usage 0, a keyboard usage above 255, or a
 * modifier out of range emits nothing and changes nothing.
 *
 * Every report that carries a key is followed in the same step by the release of its own type, and no report
 * carries a modifier without a key, so a modifier reaches the host only inside the report of the key it
 * applies to.
 */
key_engine_state_t key_engine_step(key_engine_state_t s, key_event_t ev, uint32_t now_ms, key_engine_out_t *out);

/* The HID modifier byte of the active modifiers, one-shot or locked. */
uint8_t key_engine_mods_byte(const key_engine_state_t *s);

/* The keyboard usage of F<n>: 58 to 69 for F1 to F12, 104 to 115 for F13 to F24, 0 otherwise. */
uint8_t key_usage_f(unsigned n);

/* "OFF", "ONE_SHOT" or "LOCKED". */
const char *key_mod_state_name(key_mod_state_t state);

/* "ctrl", "shift", "alt" or "win". */
const char *key_mod_name(key_mod_t mod);

#ifdef __cplusplus
}
#endif
