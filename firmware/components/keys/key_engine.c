#include "key_engine.h"

static void push(key_engine_out_t *out, key_report_t r)
{
    out->reports[out->count++] = r;
}

static key_engine_state_t clear_one_shot(key_engine_state_t s)
{
    for (unsigned i = 0; i < KEY_MOD_COUNT; i++) {
        if (s.mods[i].state == KEY_MOD_ONE_SHOT) {
            s.mods[i].state = KEY_MOD_OFF;
        }
    }
    return s;
}

static key_mod_slot_t tap_mod(key_mod_slot_t slot, uint32_t now_ms)
{
    switch (slot.state) {
    case KEY_MOD_OFF:
        slot.state = KEY_MOD_ONE_SHOT;
        slot.tapped_ms = now_ms;
        break;
    case KEY_MOD_ONE_SHOT:
        slot.state = now_ms - slot.tapped_ms <= KEY_ENGINE_DOUBLE_TAP_MS ? KEY_MOD_LOCKED : KEY_MOD_OFF;
        break;
    case KEY_MOD_LOCKED:
        slot.state = KEY_MOD_OFF;
        break;
    }
    return slot;
}

key_engine_state_t key_engine_step(key_engine_state_t s, key_event_t ev, uint32_t now_ms, key_engine_out_t *out)
{
    out->count = 0;
    switch (ev.type) {
    case KEY_EV_KEY_TAP:
    case KEY_EV_CHORD:
        if (ev.usage == 0 || ev.usage > 255) {
            break;
        }
        push(out, key_report_kbd(key_engine_mods_byte(&s) | (ev.type == KEY_EV_CHORD ? ev.mods & 0x0f : 0),
                                 (uint8_t)ev.usage));
        push(out, key_report_kbd(0, 0));
        s = clear_one_shot(s);
        break;
    case KEY_EV_MOD_TAP:
        if (ev.mod < KEY_MOD_COUNT) {
            s.mods[ev.mod] = tap_mod(s.mods[ev.mod], now_ms);
        }
        break;
    case KEY_EV_CONSUMER_TAP:
        if (ev.usage == 0) {
            break;
        }
        push(out, key_report_con(ev.usage));
        push(out, key_report_con(0));
        break;
    case KEY_EV_CLEAR_ONE_SHOT:
        s = clear_one_shot(s);
        break;
    case KEY_EV_CLEAR_MODS:
        s = KEY_ENGINE_STATE_INIT;
        break;
    case KEY_EV_RELEASE_ALL:
        push(out, key_report_kbd(0, 0));
        push(out, key_report_con(0));
        break;
    }
    return s;
}

uint8_t key_engine_mods_byte(const key_engine_state_t *s)
{
    uint8_t mods = 0;
    for (unsigned i = 0; i < KEY_MOD_COUNT; i++) {
        if (s->mods[i].state != KEY_MOD_OFF) {
            mods |= (uint8_t)(1u << i);
        }
    }
    return mods;
}

uint8_t key_usage_f(unsigned n)
{
    if (n >= 1 && n <= 12) {
        return (uint8_t)(58 + n - 1);
    }
    if (n >= 13 && n <= 24) {
        return (uint8_t)(104 + n - 13);
    }
    return 0;
}

const char *key_mod_state_name(key_mod_state_t state)
{
    switch (state) {
    case KEY_MOD_OFF:
        return "OFF";
    case KEY_MOD_ONE_SHOT:
        return "ONE_SHOT";
    case KEY_MOD_LOCKED:
        return "LOCKED";
    }
    return "?";
}

const char *key_mod_name(key_mod_t mod)
{
    switch (mod) {
    case KEY_MOD_CTRL:
        return "ctrl";
    case KEY_MOD_SHIFT:
        return "shift";
    case KEY_MOD_ALT:
        return "alt";
    case KEY_MOD_WIN:
        return "win";
    case KEY_MOD_COUNT:
        break;
    }
    return "?";
}
