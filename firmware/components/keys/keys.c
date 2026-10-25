#include "keys.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"

static struct {
    SemaphoreHandle_t lock;
    key_sink_t sink;
    key_engine_state_t state;
} s;

esp_err_t keys_start(key_sink_t sink)
{
    if (s.lock != NULL) {
        return ESP_ERR_INVALID_STATE;
    }
    if (sink.report == NULL) {
        return ESP_ERR_INVALID_ARG;
    }
    s.sink = sink;
    s.state = KEY_ENGINE_STATE_INIT;
    s.lock = xSemaphoreCreateMutex();
    return s.lock != NULL ? ESP_OK : ESP_ERR_NO_MEM;
}

bool keys_started(void)
{
    return s.lock != NULL;
}

/* The sink runs under the lock, so reports reach it in the order the engine emitted them. */
static keys_result_t step(key_event_t ev)
{
    keys_result_t res = { 0 };
    key_engine_out_t out;
    if (s.lock == NULL) {
        return res;
    }
    xSemaphoreTake(s.lock, portMAX_DELAY);
    const int64_t start = esp_timer_get_time();
    res.t = (uint32_t)(start / 1000);
    s.state = key_engine_step(s.state, ev, res.t, &out);
    for (unsigned i = 0; i < out.count; i++) {
        s.sink.report(s.sink.ctx, &out.reports[i]);
    }
    if (out.count > 0) {
        res.sink_us = (uint32_t)(esp_timer_get_time() - start);
    }
    res.reports = out.count;
    if (ev.type == KEY_EV_MOD_TAP && ev.mod < KEY_MOD_COUNT) {
        res.mod_state = s.state.mods[ev.mod].state;
    }
    xSemaphoreGive(s.lock);
    return res;
}

keys_result_t keys_tap(uint8_t usage)
{
    return step((key_event_t){ .type = KEY_EV_KEY_TAP, .usage = usage });
}

keys_result_t keys_consumer(uint16_t usage)
{
    return step((key_event_t){ .type = KEY_EV_CONSUMER_TAP, .usage = usage });
}

keys_result_t keys_mod(key_mod_t mod)
{
    return step((key_event_t){ .type = KEY_EV_MOD_TAP, .mod = mod });
}

void keys_clear_one_shot(void)
{
    step((key_event_t){ .type = KEY_EV_CLEAR_ONE_SHOT });
}

void keys_clear_all(void)
{
    step((key_event_t){ .type = KEY_EV_CLEAR_MODS });
}

key_engine_state_t keys_get_state(void)
{
    key_engine_state_t copy = KEY_ENGINE_STATE_INIT;
    if (s.lock == NULL) {
        return copy;
    }
    xSemaphoreTake(s.lock, portMAX_DELAY);
    copy = s.state;
    xSemaphoreGive(s.lock);
    return copy;
}
