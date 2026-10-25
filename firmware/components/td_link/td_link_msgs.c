#include "td_link.h"
#include "td_link_priv.h"
#include "ui.h"
#include <stdio.h>
#include <string.h>
#include "esp_app_desc.h"
#include "esp_ota_ops.h"
#include "esp_timer.h"
#include "td_recovery.h"

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
