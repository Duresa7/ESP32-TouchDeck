#include "td_link.h"
#include "td_link_priv.h"
#include "ui.h"
#include <math.h>
#include <stdio.h>
#include <string.h>
#include "esp_app_desc.h"
#include "esp_ota_ops.h"
#include "esp_timer.h"
#include "td_recovery.h"

static bool link_online(void)
{
    return td_link_state().online;
}

static void hello(const cJSON *request, cJSON *reply)
{
    (void)request;
    const esp_app_desc_t *app = esp_app_get_description();
    const esp_partition_t *running = esp_ota_get_running_partition();
    char sha[2 * sizeof app->app_elf_sha256 + 1];
    for (size_t i = 0; i < sizeof app->app_elf_sha256; i++) {
        snprintf(sha + 2 * i, 3, "%02x", app->app_elf_sha256[i]);
    }
    td_link_hello();
    ui_set_link_online(link_online);
    cJSON_AddStringToObject(reply, "product", "touchdeck");
    cJSON_AddNumberToObject(reply, "proto", 1);
    cJSON_AddStringToObject(reply, "elf_sha256", sha);
    cJSON_AddStringToObject(reply, "version", app->version);
    cJSON_AddStringToObject(reply, "running", running ? running->label : "unknown");
    const char *sinks[] = { "log", "link" };
    cJSON_AddItemToObject(reply, "sinks", cJSON_CreateStringArray(sinks, 2));
    cJSON_AddBoolToObject(reply, "recovery", td_recovery_mode());
    ui_layout_info_t layout = { 0 };
    if (ui_ready()) {
        ui_layout_info(&layout);
    }
    cJSON_AddStringToObject(reply, "layout", layout.from_storage ? "storage" : "builtin");
}

static void ping(const cJSON *request, cJSON *reply)
{
    const cJSON *n = cJSON_GetObjectItemCaseSensitive(request, "n");
    cJSON_AddNumberToObject(reply, "t", esp_timer_get_time() / 1000);
    if (cJSON_IsNumber(n) || cJSON_IsString(n) || cJSON_IsBool(n) || cJSON_IsNull(n)) {
        cJSON_AddItemToObject(reply, "n", cJSON_Duplicate(n, false));
    }
}

static void state(const cJSON *request, cJSON *reply)
{
    (void)request;
    td_link_state_t value = td_link_state();
    cJSON_AddBoolToObject(reply, "online", value.online);
    cJSON_AddNumberToObject(reply, "frames_rx", value.frames_rx);
    cJSON_AddNumberToObject(reply, "frames_tx", value.frames_tx);
    cJSON_AddNumberToObject(reply, "crc_errors", value.crc_errors);
    cJSON_AddNumberToObject(reply, "dropped", value.dropped);
    cJSON_AddNumberToObject(reply, "last_rx_ms", (double)value.last_rx_ms);
}

/* A missing optional field takes its default. A field of the wrong type is refused. */
static bool opt_string(const cJSON *request, const char *key, const char **out)
{
    const cJSON *item = cJSON_GetObjectItemCaseSensitive(request, key);
    *out = "";
    if (item == NULL) {
        return true;
    }
    if (!cJSON_IsString(item)) {
        return false;
    }
    *out = item->valuestring;
    return true;
}

static bool opt_bar(const cJSON *item, float *out)
{
    *out = -1.0f;
    if (item == NULL || cJSON_IsNull(item)) {
        return true;
    }
    if (!cJSON_IsNumber(item)) {
        return false;
    }
    *out = (float)item->valuedouble;
    return true;
}

static void widget_set(const cJSON *request, cJSON *reply)
{
    const cJSON *tile = cJSON_GetObjectItemCaseSensitive(request, "tile");
    const cJSON *stale = cJSON_GetObjectItemCaseSensitive(request, "stale");
    const char *value = NULL;
    const char *caption = NULL;
    float bar = -1.0f;
    if (!cJSON_IsString(tile)) {
        td_link_fail(reply, "tile must be a string");
        return;
    }
    if (!opt_string(request, "value", &value) || !opt_string(request, "caption", &caption)) {
        td_link_fail(reply, "value and caption must be strings");
        return;
    }
    if (!opt_bar(cJSON_GetObjectItemCaseSensitive(request, "bar"), &bar)) {
        td_link_fail(reply, "bar must be a number");
        return;
    }
    if (stale != NULL && !cJSON_IsBool(stale)) {
        td_link_fail(reply, "stale must be a boolean");
        return;
    }
    ui_widget_state_t stored;
    const char *error = ui_widget_update(tile->valuestring, value, caption, bar, cJSON_IsTrue(stale), &stored);
    if (error != NULL) {
        td_link_fail(reply, error);
        return;
    }
    cJSON_AddStringToObject(reply, "tile", tile->valuestring);
    cJSON_AddStringToObject(reply, "value", stored.value);
    cJSON_AddStringToObject(reply, "caption", stored.caption);
    cJSON_AddNumberToObject(reply, "bar", stored.bar < 0.0f ? -1.0 : round(stored.bar * 1000.0) / 1000.0);
    cJSON_AddBoolToObject(reply, "stale", stored.stale);
}

static void dashboard_set(const cJSON *request, cJSON *reply)
{
    static ui_dash_row_t rows[UI_DASH_ROWS_MAX];
    const cJSON *page = cJSON_GetObjectItemCaseSensitive(request, "page");
    const cJSON *list = cJSON_GetObjectItemCaseSensitive(request, "rows");
    if (!cJSON_IsString(page)) {
        td_link_fail(reply, "page must be a string");
        return;
    }
    if (!cJSON_IsArray(list)) {
        td_link_fail(reply, "rows must be an array");
        return;
    }
    const int count = cJSON_GetArraySize(list);
    if (count > UI_DASH_ROWS_MAX) {
        td_link_fail(reply, "too many rows");
        return;
    }
    int i = 0;
    const cJSON *row = NULL;
    cJSON_ArrayForEach(row, list)
    {
        const char *label = NULL;
        const char *value = NULL;
        if (!cJSON_IsObject(row) || !opt_string(row, "label", &label) || !opt_string(row, "value", &value) ||
            !opt_bar(cJSON_GetObjectItemCaseSensitive(row, "bar"), &rows[i].bar)) {
            td_link_fail(reply, "each row is an object with string label and value and a number bar");
            return;
        }
        ui_widget_copy_text(rows[i].label, sizeof rows[i].label, label);
        ui_widget_copy_text(rows[i].value, sizeof rows[i].value, value);
        i++;
    }
    const char *error = ui_dashboard_update(page->valuestring, rows, (size_t)count);
    if (error != NULL) {
        td_link_fail(reply, error);
        return;
    }
    cJSON_AddStringToObject(reply, "page", page->valuestring);
    cJSON_AddNumberToObject(reply, "rows", count);
}

static const struct {
    const char *name;
    void (*handler)(const cJSON *request, cJSON *reply);
} s_messages[] = {
    { "hello", hello },
    { "ping", ping },
    { "link.state", state },
    { "layout.put", td_link_layout_put },
    { "layout.get", td_link_layout_get },
    { "icon.put", td_link_icon_put },
    { "icon.list", td_link_icon_list },
    { "icon.rm", td_link_icon_rm },
    { "settings.get", td_link_settings_get },
    { "settings.put", td_link_settings_put },
    { "store.stat", td_link_store_stat },
    { "xfer.end", td_link_xfer_end },
    { "update.begin", td_link_update_begin },
    { "update.end", td_link_update_end },

    { "widget.set", widget_set },
    { "dashboard.set", dashboard_set },
};

#define ECHO_CHARS_MAX 64

static void echo_name(const char *name, char out[4 * ECHO_CHARS_MAX + 1])
{
    size_t len = 0;
    size_t chars = 0;
    while (name[len] && len < 4 * ECHO_CHARS_MAX) {
        if (((unsigned char)name[len] & 0xc0) != 0x80 && chars++ == ECHO_CHARS_MAX) {
            break;
        }
        len++;
    }
    memcpy(out, name, len);
    out[len] = '\0';
}

cJSON *td_link_dispatch(const cJSON *request)
{
    const cJSON *message = cJSON_GetObjectItemCaseSensitive(request, "m");
    if (!cJSON_IsString(message)) {
        return NULL;
    }
    cJSON *reply = cJSON_CreateObject();
    if (!reply) {
        return NULL;
    }
    char name[4 * ECHO_CHARS_MAX + 1];
    echo_name(message->valuestring, name);
    cJSON_AddStringToObject(reply, "m", name);
    for (size_t i = 0; i < sizeof s_messages / sizeof s_messages[0]; i++) {
        if (strcmp(message->valuestring, s_messages[i].name) == 0) {
            cJSON_AddBoolToObject(reply, "ok", true);
            s_messages[i].handler(request, reply);
            return reply;
        }
    }
    cJSON_AddBoolToObject(reply, "ok", false);
    cJSON_AddStringToObject(reply, "error", "unknown message");
    return reply;
}
