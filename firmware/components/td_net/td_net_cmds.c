#include "td_net_priv.h"
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "esp_wifi.h"

void td_net_state_fields(cJSON *reply)
{
    const td_net_state_t s = td_net_state();
    cJSON_AddBoolToObject(reply, "configured", s.configured);
    cJSON_AddStringToObject(reply, "ssid", s.ssid);
    cJSON_AddStringToObject(reply, "state", td_net_status_name(s.state));
    cJSON_AddItemToObject(reply, "ip", s.ip[0] ? cJSON_CreateString(s.ip) : cJSON_CreateNull());
    cJSON_AddItemToObject(reply, "rssi", s.ip[0] ? cJSON_CreateNumber(s.rssi) : cJSON_CreateNull());
    cJSON_AddItemToObject(reply, "reason", s.reason[0] ? cJSON_CreateString(s.reason) : cJSON_CreateNull());
    cJSON_AddStringToObject(reply, "host", s.host);
}

void td_net_hello(cJSON *reply)
{
    const td_net_state_t s = td_net_state();
    cJSON *wifi = cJSON_AddObjectToObject(reply, "wifi");
    cJSON_AddStringToObject(wifi, "state", td_net_status_name(s.state));
    cJSON_AddItemToObject(wifi, "ip", s.ip[0] ? cJSON_CreateString(s.ip) : cJSON_CreateNull());
}

static void failure(cJSON *reply, esp_err_t err)
{
    const char *reason = "wifi";
    const char *error = "wifi driver error";
    if (err == ESP_ERR_INVALID_ARG) {
        reason = "args";
        error = "ssid must be 1 to 32 bytes of UTF-8 and password empty or 8 to 63 bytes";
    } else if (err == ESP_ERR_INVALID_STATE || err == ESP_ERR_WIFI_STATE || err == ESP_ERR_WIFI_CONN) {
        reason = "busy";
        error = "wifi is busy";
    }
    cJSON_ReplaceItemInObjectCaseSensitive(reply, "ok", cJSON_CreateFalse());
    cJSON_AddStringToObject(reply, "error", error);
    cJSON_AddStringToObject(reply, "reason", reason);
}

static void execute(const char *op, const char *ssid, const char *password, cJSON *reply)
{
    esp_err_t err = ESP_OK;
    if (strcmp(op, "scan") == 0) {
        err = td_net_scan(reply);
    } else {
        if (strcmp(op, "set") == 0) err = td_net_set(ssid, password);
        else if (strcmp(op, "forget") == 0) err = td_net_forget();
        if (err == ESP_OK) td_net_state_fields(reply);
    }
    if (err != ESP_OK) failure(reply, err);
}

void td_net_message(const cJSON *request, cJSON *reply)
{
    const cJSON *m = cJSON_GetObjectItemCaseSensitive(request, "m");
    const cJSON *ssid = cJSON_GetObjectItemCaseSensitive(request, "ssid");
    const cJSON *password = cJSON_GetObjectItemCaseSensitive(request, "password");
    const char *op = m->valuestring + strlen("wifi.");
    execute(op, cJSON_IsString(ssid) ? ssid->valuestring : NULL,
            cJSON_IsString(password) ? password->valuestring : NULL, reply);
}

static int command(int argc, char **argv)
{
    cJSON *reply = cJSON_CreateObject();
    cJSON_AddStringToObject(reply, "cmd", "wifi");
    cJSON_AddBoolToObject(reply, "ok", true);
    if (argc == 4 && strcmp(argv[1], "set") == 0) {
        execute("set", argv[2], argv[3], reply);
        memset(argv[3], 0, strlen(argv[3]));
    } else if (argc == 2 && (strcmp(argv[1], "state") == 0 || strcmp(argv[1], "forget") == 0 ||
                            strcmp(argv[1], "scan") == 0)) {
        execute(argv[1], NULL, NULL, reply);
    } else {
        failure(reply, ESP_ERR_INVALID_ARG);
        cJSON_ReplaceItemInObjectCaseSensitive(reply, "error",
            cJSON_CreateString("usage: wifi state | set <ssid> <password> | forget | scan"));
    }
    char *json = cJSON_PrintUnformatted(reply);
    if (json) {
        char line[1028];
        snprintf(line, sizeof line, "@td %s", json);
        td_console_write_line(line);
    }
    cJSON_free(json);
    cJSON_Delete(reply);
    return 0;
}

const esp_console_cmd_t *td_net_console_commands(size_t *count)
{
    static const esp_console_cmd_t commands[] = {
        { .command = "wifi", .help = "manage the stored Wi-Fi station", .func = command },
    };
    *count = sizeof commands / sizeof commands[0];
    return commands;
}
