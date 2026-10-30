#include "td_link_priv.h"
#include <math.h>
#include <string.h>
#include "esp_log.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "power.h"
#include "td_device.h"
#include "ui.h"

static const char *TAG = "td_link";

/* The idle settings in force. The boot and v2.idle.set write them, and the link task reads them for every picture. */
static portMUX_TYPE s_lock = portMUX_INITIALIZER_UNLOCKED;
static td_idle_settings_t s_idle = {
    .enabled = false, .timeout_s = TD_IDLE_TIMEOUT_DEFAULT_S, .interval_s = TD_IDLE_INTERVAL_DEFAULT_S,
};

static void apply(const td_idle_settings_t *settings)
{
    portENTER_CRITICAL(&s_lock);
    s_idle = *settings;
    portEXIT_CRITICAL(&s_lock);
    const esp_err_t err = power_set_photo_idle(settings->enabled, settings->timeout_s, esp_timer_get_time());
    if (err != ESP_OK) {
        ESP_LOGW(TAG, "photo idle mode not applied: %s", esp_err_to_name(err));
    }
    ui_idle_set_interval(settings->interval_s);
}

void td_link_idle_boot(void)
{
    td_idle_settings_t settings;
    const esp_err_t err = td_device_idle_get(&settings);
    if (err != ESP_OK) {
        ESP_LOGW(TAG, "idle settings unreadable, photo frame off: %s", esp_err_to_name(err));
    }
    apply(&settings);
}

uint32_t td_link_idle_interval_s(void)
{
    portENTER_CRITICAL(&s_lock);
    const uint32_t interval_s = s_idle.interval_s;
    portEXIT_CRITICAL(&s_lock);
    return interval_s;
}

/* A whole JSON number from min to max. */
static bool whole(const cJSON *request, const char *key, double min, double max, double *out)
{
    const cJSON *item = cJSON_GetObjectItemCaseSensitive(request, key);
    if (!cJSON_IsNumber(item) || item->valuedouble < min || item->valuedouble > max
        || item->valuedouble != floor(item->valuedouble)) {
        return false;
    }
    *out = item->valuedouble;
    return true;
}

static void idle_fields(cJSON *reply, const td_idle_settings_t *settings)
{
    cJSON_AddBoolToObject(reply, "enabled", settings->enabled);
    cJSON_AddNumberToObject(reply, "timeout_s", settings->timeout_s);
    cJSON_AddNumberToObject(reply, "interval_s", settings->interval_s);
    cJSON_AddStringToObject(reply, "fallback", "clock");
}

void td_link_idle_get(const cJSON *request, cJSON *reply)
{
    td_idle_settings_t settings;
    if (!td_link_v2_fields(request, NULL, 0)) {
        td_link_v2_fail(reply, "args", "Invalid request fields");
    } else if (td_device_idle_get(&settings) != ESP_OK) {
        td_link_v2_fail(reply, "storage", "Idle settings unavailable");
    } else {
        idle_fields(reply, &settings);
    }
}

void td_link_idle_set(const cJSON *request, cJSON *reply)
{
    static const char *const keys[] = { "enabled", "timeout_s", "interval_s", "fallback" };
    const cJSON *enabled = cJSON_GetObjectItemCaseSensitive(request, "enabled");
    const char *fallback = cJSON_GetStringValue(cJSON_GetObjectItemCaseSensitive(request, "fallback"));
    double timeout_s;
    double interval_s;
    if (!td_link_v2_fields(request, keys, 4) || !cJSON_IsBool(enabled) || fallback == NULL
        || strcmp(fallback, "clock") != 0
        || !whole(request, "timeout_s", TD_IDLE_TIMEOUT_MIN_S, TD_IDLE_TIMEOUT_MAX_S, &timeout_s)
        || !whole(request, "interval_s", TD_IDLE_INTERVAL_MIN_S, TD_IDLE_INTERVAL_MAX_S, &interval_s)) {
        td_link_v2_fail(reply, "args", "Invalid request fields");
        return;
    }
    const td_idle_settings_t settings = {
        .enabled = cJSON_IsTrue(enabled), .timeout_s = (uint32_t)timeout_s, .interval_s = (uint32_t)interval_s,
    };
    if (td_device_idle_set(&settings) != ESP_OK) {
        td_link_v2_fail(reply, "storage", "Idle settings not saved");
        return;
    }
    apply(&settings);
    idle_fields(reply, &settings);
}

void td_link_clock_set(const cJSON *request, cJSON *reply)
{
    static const char *const keys[] = { "unix_s", "offset_min" };
    double unix_s;
    double offset_min;
    if (!td_link_v2_fields(request, keys, 2) || !whole(request, "unix_s", 0, (double)TD_CLOCK_UNIX_MAX_S, &unix_s)
        || !whole(request, "offset_min", -TD_CLOCK_OFFSET_MAX_MIN, TD_CLOCK_OFFSET_MAX_MIN, &offset_min)) {
        td_link_v2_fail(reply, "args", "Invalid request fields");
        return;
    }
    ui_idle_set_clock((uint64_t)unix_s, (int32_t)offset_min);
    cJSON_AddNumberToObject(reply, "unix_s", unix_s);
    cJSON_AddNumberToObject(reply, "offset_min", offset_min);
}
