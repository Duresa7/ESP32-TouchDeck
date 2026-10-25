#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "esp_console.h"
#include "esp_err.h"
#include "key_engine.h"
#include "key_sink.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct key_sink_log key_sink_log_t;

typedef struct {
    uint32_t t;                 /* the engine's time of the event, milliseconds since boot */
    uint8_t reports;            /* reports the event emitted */
    uint32_t sink_us;           /* from the event to the sink taking the last report, 0 when it emitted none */
    key_mod_state_t mod_state;  /* keys_mod only: the modifier's state after the tap */
} keys_result_t;

/*
 * The key engine on the board: the reducer's state, the clock, and the one sink, behind one lock, so the pages
 * and the console can call from their own tasks. Call it once before td_console_start. The sink is opaque here.
 * In milestone 1 it is the console key log, and the USB HID sink plugs in behind the same key_sink_t.
 */
esp_err_t keys_start(key_sink_t sink);

bool keys_started(void);

/* A keyboard usage, 1 to 255, with the active modifiers, then the all-released report. Clears one-shot modifiers. */
keys_result_t keys_tap(uint8_t usage);

/* A consumer usage, 1 to 65535, then the empty consumer report. Leaves every modifier as it is. */
keys_result_t keys_consumer(uint16_t usage);

/* One tap on a modifier: OFF to ONE_SHOT, a quick second tap to LOCKED, a late one or a tap on LOCKED to OFF. */
keys_result_t keys_mod(key_mod_t mod);

/* For a page swipe: one-shot modifiers return to OFF, locked ones stay. Emits nothing. */
void keys_clear_one_shot(void);

/* For the idle screen-off: every modifier returns to OFF. Emits nothing. */
void keys_clear_all(void);

/* A copy of the engine state, for the modifier tiles' three looks. */
key_engine_state_t keys_get_state(void);

/* The keys console commands, for td_console_register(). They drive the engine and control the key log sink. */
const esp_console_cmd_t *keys_console_commands(key_sink_log_t *log, size_t *count);

#ifdef __cplusplus
}
#endif
