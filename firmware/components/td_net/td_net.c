#include "td_net_priv.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "esp_event.h"
#include "esp_netif.h"
#include "esp_timer.h"
#include "esp_wifi.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"
#include "mdns.h"
#include "nvs.h"
#include "nvs_flash.h"

ESP_EVENT_DEFINE_BASE(TD_NET_EVENT);
enum { APPLY, RETRY };

static SemaphoreHandle_t s_lock;
static SemaphoreHandle_t s_scan_lock;
static td_net_state_t s_state;
static wifi_config_t s_config;
static nvs_handle_t s_nvs;
static esp_timer_handle_t s_retry;
static unsigned s_attempt;
static bool s_ready;
static bool s_scanning;

static void lock(void) { xSemaphoreTake(s_lock, portMAX_DELAY); }
static void unlock(void) { xSemaphoreGive(s_lock); }

static void clear_ip(void)
{
    s_state.ip[0] = '\0';
    s_state.rssi = 0;
}

static void retry_later(void)
{
    esp_timer_stop(s_retry);
    const unsigned delay = td_net_retry_seconds(s_attempt);
    if (s_attempt < 4) {
        s_attempt++;
    }
    esp_timer_start_once(s_retry, (uint64_t)delay * 1000000);
}

static void retry_timer(void *arg)
{
    (void)arg;
    if (esp_event_post(TD_NET_EVENT, RETRY, NULL, 0, 0) != ESP_OK) {
        esp_timer_start_once(s_retry, 1000000);
    }
}

static const char *disconnect_reason(unsigned reason)
{
    switch (reason) {
    case WIFI_REASON_NO_AP_FOUND: return "no_ap_found";
    case WIFI_REASON_AUTH_FAIL: return "auth_fail";
    case WIFI_REASON_AUTH_EXPIRE: return "auth_expire";
    case WIFI_REASON_ASSOC_FAIL: return "assoc_fail";
    case WIFI_REASON_HANDSHAKE_TIMEOUT: return "handshake_timeout";
    case WIFI_REASON_4WAY_HANDSHAKE_TIMEOUT: return "handshake_timeout";
    case WIFI_REASON_BEACON_TIMEOUT: return "beacon_timeout";
    case WIFI_REASON_ASSOC_LEAVE: return "assoc_leave";
    default: return "disconnected";
    }
}

static void event(void *arg, esp_event_base_t base, int32_t id, void *data)
{
    (void)arg;
    lock();
    if (base == TD_NET_EVENT) {
        if (id == APPLY) {
            esp_timer_stop(s_retry);
            esp_wifi_disconnect();
            if (s_state.configured) {
                esp_err_t err = esp_wifi_set_config(WIFI_IF_STA, &s_config);
                if (err == ESP_OK) {
                    err = esp_wifi_connect();
                }
                if (err != ESP_OK) {
                    s_state.state = TD_NET_FAILED;
                    snprintf(s_state.reason, sizeof s_state.reason, "driver");
                    retry_later();
                }
            }
        } else if (s_state.configured && s_state.state != TD_NET_CONNECTED) {
            if (s_scanning) {
                esp_timer_start_once(s_retry, 1000000);
            } else {
                s_state.state = TD_NET_CONNECTING;
                if (esp_wifi_connect() != ESP_OK) {
                    s_state.state = TD_NET_FAILED;
                    retry_later();
                }
            }
        }
    } else if (base == WIFI_EVENT && id == WIFI_EVENT_STA_START) {
        if (s_state.configured && esp_wifi_connect() != ESP_OK) {
            s_state.state = TD_NET_FAILED;
            retry_later();
        }
    } else if (base == WIFI_EVENT && id == WIFI_EVENT_STA_DISCONNECTED) {
        const wifi_event_sta_disconnected_t *d = data;
        if (s_state.configured && d->ssid_len == strlen(s_state.ssid) &&
            memcmp(d->ssid, s_state.ssid, d->ssid_len) == 0 &&
            !(d->reason == WIFI_REASON_ASSOC_LEAVE && s_state.state == TD_NET_CONNECTING)) {
            clear_ip();
            s_state.state = TD_NET_FAILED;
            snprintf(s_state.reason, sizeof s_state.reason, "%s", disconnect_reason(d->reason));
            retry_later();
        }
    } else if (base == IP_EVENT && id == IP_EVENT_STA_GOT_IP && s_state.configured) {
        wifi_ap_record_t ap;
        if (esp_wifi_sta_get_ap_info(&ap) == ESP_OK &&
            memcmp(ap.ssid, s_state.ssid, strlen(s_state.ssid)) == 0) {
            const ip_event_got_ip_t *got = data;
            snprintf(s_state.ip, sizeof s_state.ip, IPSTR, IP2STR(&got->ip_info.ip));
            s_state.state = TD_NET_CONNECTED;
            s_state.rssi = ap.rssi;
            s_state.reason[0] = '\0';
            s_attempt = 0;
            esp_timer_stop(s_retry);
        }
    } else if (base == IP_EVENT && id == IP_EVENT_STA_LOST_IP && s_state.configured) {
        clear_ip();
        s_state.state = TD_NET_FAILED;
        snprintf(s_state.reason, sizeof s_state.reason, "lost_ip");
        esp_wifi_disconnect();
        retry_later();
    }
    unlock();
}

td_net_state_t td_net_state(void)
{
    if (!s_ready) {
        return (td_net_state_t){0};
    }
    lock();
    td_net_state_t value = s_state;
    unlock();
    if (value.state == TD_NET_CONNECTED) {
        wifi_ap_record_t ap;
        if (esp_wifi_sta_get_ap_info(&ap) == ESP_OK) {
            value.rssi = ap.rssi;
        }
    }
    return value;
}

esp_err_t td_net_set(const char *ssid, const char *password)
{
    if (!td_net_credentials_valid(ssid, password)) {
        return ESP_ERR_INVALID_ARG;
    }
    if (!s_ready) {
        return ESP_ERR_INVALID_STATE;
    }
    lock();
    esp_err_t err = nvs_set_str(s_nvs, "ssid", ssid);
    if (err == ESP_OK) err = nvs_set_str(s_nvs, "pass", password);
    if (err == ESP_OK) err = nvs_commit(s_nvs);
    if (err == ESP_OK) {
        memset(&s_config, 0, sizeof s_config);
        memcpy(s_config.sta.ssid, ssid, strlen(ssid));
        memcpy(s_config.sta.password, password, strlen(password));
        s_config.sta.pmf_cfg.capable = true;
        snprintf(s_state.ssid, sizeof s_state.ssid, "%s", ssid);
        s_state.configured = true;
        s_state.state = TD_NET_CONNECTING;
        s_state.reason[0] = '\0';
        clear_ip();
        s_attempt = 0;
    }
    unlock();
    if (err == ESP_OK) err = esp_event_post(TD_NET_EVENT, APPLY, NULL, 0, pdMS_TO_TICKS(100));
    return err;
}

esp_err_t td_net_forget(void)
{
    if (!s_ready) return ESP_ERR_INVALID_STATE;
    lock();
    esp_err_t err = nvs_erase_all(s_nvs);
    if (err == ESP_OK) err = nvs_commit(s_nvs);
    if (err == ESP_OK) {
        memset(&s_config, 0, sizeof s_config);
        s_state.configured = false;
        s_state.state = TD_NET_OFF;
        s_state.ssid[0] = '\0';
        s_state.reason[0] = '\0';
        clear_ip();
    }
    unlock();
    if (err == ESP_OK) err = esp_event_post(TD_NET_EVENT, APPLY, NULL, 0, pdMS_TO_TICKS(100));
    return err;
}

esp_err_t td_net_mdns_txt_set(const char *key, const char *value)
{
    if (!key || !*key || !value) return ESP_ERR_INVALID_ARG;
    if (!s_ready) return ESP_ERR_INVALID_STATE;
    return mdns_service_txt_item_set("_touchdeck", "_tcp", key, value);
}

esp_err_t td_net_scan(cJSON *reply)
{
    if (!s_ready) return ESP_ERR_INVALID_STATE;
    if (xSemaphoreTake(s_scan_lock, 0) != pdTRUE) return ESP_ERR_INVALID_STATE;
    lock();
    s_scanning = true;
    unlock();
    const wifi_scan_config_t config = { .show_hidden = false, .scan_time.active = { .min = 40, .max = 100 } };
    esp_err_t err = esp_wifi_scan_start(&config, true);
    uint16_t count = 16;
    wifi_ap_record_t *aps = calloc(count, sizeof *aps);
    if (err == ESP_OK && !aps) err = ESP_ERR_NO_MEM;
    if (err == ESP_OK) err = esp_wifi_scan_get_ap_records(&count, aps);
    if (err == ESP_OK) {
        cJSON *list = cJSON_AddArrayToObject(reply, "aps");
        for (unsigned i = 0; list && i < count; i++) {
            char ssid[33];
            memcpy(ssid, aps[i].ssid, 32);
            ssid[32] = '\0';
            cJSON *ap = cJSON_CreateObject();
            cJSON_AddItemToArray(list, ap);
            cJSON_AddStringToObject(ap, "ssid", ssid);
            cJSON_AddNumberToObject(ap, "rssi", aps[i].rssi);
            cJSON_AddNumberToObject(ap, "auth", aps[i].authmode);
            char *json = cJSON_PrintUnformatted(reply);
            const bool fits = json && strlen(json) <= 1000;
            cJSON_free(json);
            if (!fits) {
                cJSON_DeleteItemFromArray(list, i);
                break;
            }
        }
    }
    esp_wifi_clear_ap_list();
    free(aps);
    lock();
    s_scanning = false;
    unlock();
    xSemaphoreGive(s_scan_lock);
    return err;
}

esp_err_t td_net_start(void)
{
    if (s_ready) return ESP_ERR_INVALID_STATE;
    s_lock = xSemaphoreCreateMutex();
    s_scan_lock = xSemaphoreCreateMutex();
    if (!s_lock || !s_scan_lock) return ESP_ERR_NO_MEM;
    esp_err_t err = nvs_flash_init();
    if (err != ESP_OK) return err;
    err = nvs_open("td_net", NVS_READWRITE, &s_nvs);
    if (err != ESP_OK) return err;
    char ssid[33] = "";
    char password[64] = "";
    size_t sn = sizeof ssid, pn = sizeof password;
    if (nvs_get_str(s_nvs, "ssid", ssid, &sn) == ESP_OK &&
        nvs_get_str(s_nvs, "pass", password, &pn) == ESP_OK &&
        td_net_credentials_valid(ssid, password)) {
        s_state.configured = true;
        s_state.state = TD_NET_CONNECTING;
        memcpy(s_state.ssid, ssid, sn);
        memcpy(s_config.sta.ssid, ssid, sn - 1);
        memcpy(s_config.sta.password, password, pn - 1);
        s_config.sta.pmf_cfg.capable = true;
    }
    memset(password, 0, sizeof password);
    err = esp_netif_init();
    if (err != ESP_OK) return err;
    err = esp_event_loop_create_default();
    if (err != ESP_OK && err != ESP_ERR_INVALID_STATE) return err;
    esp_netif_t *netif = esp_netif_create_default_wifi_sta();
    if (!netif) return ESP_ERR_NO_MEM;
    wifi_init_config_t init = WIFI_INIT_CONFIG_DEFAULT();
    init.nvs_enable = false;
    err = esp_wifi_init(&init);
    if (err != ESP_OK) return err;
    uint8_t mac[6];
    err = esp_wifi_get_mac(WIFI_IF_STA, mac);
    if (err != ESP_OK) return err;
    snprintf(s_state.host, sizeof s_state.host, "touchdeck-%02x%02x%02x", mac[3], mac[4], mac[5]);
    esp_netif_set_hostname(netif, s_state.host);
    char identity[13];
    snprintf(identity, sizeof identity, "%02x%02x%02x%02x%02x%02x", mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
    err = mdns_init();
    if (err != ESP_OK) return err;
    err = mdns_hostname_set(s_state.host);
    if (err != ESP_OK) return err;
    mdns_txt_item_t txt[] = { { "id", identity }, { "proto", "1" }, { "name", s_state.host } };
    err = mdns_service_add(s_state.host, "_touchdeck", "_tcp", 47820, txt, 3);
    if (err != ESP_OK) return err;
    const esp_timer_create_args_t timer = { .callback = retry_timer, .name = "wifi_retry" };
    err = esp_timer_create(&timer, &s_retry);
    if (err != ESP_OK) return err;
    err = esp_event_handler_register(TD_NET_EVENT, ESP_EVENT_ANY_ID, event, NULL);
    if (err == ESP_OK) err = esp_event_handler_register(WIFI_EVENT, ESP_EVENT_ANY_ID, event, NULL);
    if (err == ESP_OK) err = esp_event_handler_register(IP_EVENT, ESP_EVENT_ANY_ID, event, NULL);
    if (err == ESP_OK) err = esp_wifi_set_storage(WIFI_STORAGE_RAM);
    if (err == ESP_OK) err = esp_wifi_set_mode(WIFI_MODE_STA);
    if (err == ESP_OK && s_state.configured) err = esp_wifi_set_config(WIFI_IF_STA, &s_config);
    if (err == ESP_OK) err = esp_wifi_start();
    if (err == ESP_OK) err = esp_wifi_set_ps(WIFI_PS_MIN_MODEM);
    if (err == ESP_OK) s_ready = true;
    return err;
}
