#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "cJSON.h"
#include "td_console.h"

typedef enum {
    TD_NET_OFF,
    TD_NET_CONNECTING,
    TD_NET_CONNECTED,
    TD_NET_FAILED,
} td_net_status_t;

typedef struct {
    bool configured;
    char ssid[33];
    td_net_status_t state;
    char ip[16];
    int rssi;
    char reason[32];
    char host[24];
} td_net_state_t;

esp_err_t td_net_start(void);
td_net_state_t td_net_state(void);
const char *td_net_status_name(td_net_status_t state);
bool td_net_credentials_valid(const char *ssid, const char *password);
unsigned td_net_retry_seconds(unsigned attempt);
esp_err_t td_net_set(const char *ssid, const char *password);
esp_err_t td_net_forget(void);

/* Adds or replaces a TXT item on _touchdeck._tcp. Call after td_net_start. */
esp_err_t td_net_mdns_txt_set(const char *key, const char *value);

const esp_console_cmd_t *td_net_console_commands(size_t *count);
void td_net_state_fields(cJSON *reply);
void td_net_hello(cJSON *reply);
void td_net_message(const cJSON *request, cJSON *reply);
