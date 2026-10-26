#include "td_net_priv.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "esp_event.h"
#include "esp_log.h"
#include "esp_netif.h"
#include "esp_timer.h"
#include "esp_wifi.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"
#include "mdns.h"
#include "nvs.h"
#include "nvs_flash.h"

#define SCAN_MAX 16
#define SCAN_REPLY_MAX 1000

static const char *TAG = "td_net";

ESP_EVENT_DEFINE_BASE(TD_NET_EVENT);
enum { APPLY, RETRY };

/* s_lock guards the state, the join config and the retry count, and is held only for a few instructions and the
 * driver calls of one event. s_store_lock serializes set and forget, so flash writes happen outside s_lock and the
 * UI's status read never waits for one. */
static SemaphoreHandle_t s_lock;
static SemaphoreHandle_t s_store_lock;
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

static void fail(const char *reason)
{
    clear_ip();
    s_state.state = TD_NET_FAILED;
    snprintf(s_state.reason, sizeof s_state.reason, "%s", reason);
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

static void build_config(const char *ssid, const char *password)
{
    memset(&s_config, 0, sizeof s_config);
    memcpy(s_config.sta.ssid, ssid, strlen(ssid));
    memcpy(s_config.sta.password, password, strlen(password));
    s_config.sta.pmf_cfg.capable = true;
    s_config.sta.sae_pwe_h2e = WPA3_SAE_PWE_BOTH;
    s_config.sta.threshold.authmode = password[0] ? WIFI_AUTH_WPA2_PSK : WIFI_AUTH_OPEN;
}

/* Starts one join attempt with the stored network. The caller holds s_lock and has checked that one is stored. */
static void join(void)
{
    esp_err_t err = esp_wifi_set_config(WIFI_IF_STA, &s_config);
    if (err == ESP_OK) {
        err = esp_wifi_connect();
    }
    if (err == ESP_OK || err == ESP_ERR_WIFI_CONN) {
        s_state.state = TD_NET_CONNECTING;
    } else {
        fail("driver");
        retry_later();
    }
}

static bool same_ssid(const uint8_t *ssid, size_t len)
{
    return len == strlen(s_state.ssid) && memcmp(ssid, s_state.ssid, len) == 0;
}

static void event(void *arg, esp_event_base_t base, int32_t id, void *data)
{
    (void)arg;
    const char *note = NULL;
    char reason[32] = "";
    lock();
    if (base == TD_NET_EVENT) {
        if (id == APPLY) {
            esp_timer_stop(s_retry);
            esp_wifi_disconnect();
            if (s_state.configured) {
                join();
            }
        } else if (s_state.configured && s_state.state != TD_NET_CONNECTED) {
            if (s_scanning) {
                esp_timer_start_once(s_retry, 1000000);
            } else {
                join();
            }
        }
    } else if (base == WIFI_EVENT && id == WIFI_EVENT_STA_START) {
        if (s_state.configured) {
            join();
        }
    } else if (base == WIFI_EVENT && id == WIFI_EVENT_STA_DISCONNECTED) {
        const wifi_event_sta_disconnected_t *d = data;
        /* A join that replaces another one first disconnects the old one, and that event lands after the new join
         * began. It is not a failure of the new network. */
        const bool stale = d->ssid_len != 0 && !same_ssid(d->ssid, d->ssid_len);
        const bool ours = d->reason == WIFI_REASON_ASSOC_LEAVE && s_state.state == TD_NET_CONNECTING;
        if (s_state.configured && !stale && !ours) {
            fail(td_net_reason_word(d->reason));
            snprintf(reason, sizeof reason, "%s", s_state.reason);
            note = "disconnected";
            retry_later();
        }
    } else if (base == IP_EVENT && id == IP_EVENT_STA_GOT_IP && s_state.configured) {
        const ip_event_got_ip_t *got = data;
        wifi_ap_record_t ap;
        snprintf(s_state.ip, sizeof s_state.ip, IPSTR, IP2STR(&got->ip_info.ip));
        s_state.rssi = esp_wifi_sta_get_ap_info(&ap) == ESP_OK ? ap.rssi : 0;
        s_state.state = TD_NET_CONNECTED;
        s_state.reason[0] = '\0';
        s_attempt = 0;
        esp_timer_stop(s_retry);
        snprintf(reason, sizeof reason, "%s", s_state.ip);
        note = "connected";
    } else if (base == IP_EVENT && id == IP_EVENT_STA_LOST_IP && s_state.state == TD_NET_CONNECTED) {
        fail("lost_ip");
        snprintf(reason, sizeof reason, "lost_ip");
        note = "disconnected";
        esp_wifi_disconnect();
        retry_later();
    }
    unlock();
    if (note) {
        ESP_LOGI(TAG, "%s %s", note, reason);
    }
}

td_net_status_t td_net_status(void)
{
    if (!s_ready) {
        return TD_NET_OFF;
    }
    lock();
    const td_net_status_t value = s_state.state;
    unlock();
    return value;
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
    xSemaphoreTake(s_store_lock, portMAX_DELAY);
    esp_err_t err = nvs_set_str(s_nvs, "ssid", ssid);
    if (err == ESP_OK) err = nvs_set_str(s_nvs, "pass", password);
    if (err == ESP_OK) err = nvs_commit(s_nvs);
    if (err == ESP_OK) {
        lock();
        build_config(ssid, password);
        snprintf(s_state.ssid, sizeof s_state.ssid, "%s", ssid);
        s_state.configured = true;
        s_state.state = TD_NET_CONNECTING;
        s_state.reason[0] = '\0';
        clear_ip();
        s_attempt = 0;
        unlock();
        err = esp_event_post(TD_NET_EVENT, APPLY, NULL, 0, pdMS_TO_TICKS(100));
        if (err != ESP_OK) {
            lock();
            retry_later();
            unlock();
        }
    }
    xSemaphoreGive(s_store_lock);
    return err;
}

esp_err_t td_net_forget(void)
{
    if (!s_ready) return ESP_ERR_INVALID_STATE;
    xSemaphoreTake(s_store_lock, portMAX_DELAY);
    esp_err_t err = nvs_erase_key(s_nvs, "ssid");
    esp_err_t pass_err = nvs_erase_key(s_nvs, "pass");
    if (err == ESP_ERR_NVS_NOT_FOUND) err = ESP_OK;
    if (err == ESP_OK && pass_err != ESP_ERR_NVS_NOT_FOUND) err = pass_err;
    if (err == ESP_OK) err = nvs_commit(s_nvs);
    if (err == ESP_OK) {
        lock();
        memset(&s_config, 0, sizeof s_config);
        s_state.configured = false;
        s_state.state = TD_NET_OFF;
        s_state.ssid[0] = '\0';
        s_state.reason[0] = '\0';
        clear_ip();
        s_attempt = 0;
        esp_timer_stop(s_retry);
        unlock();
        err = esp_event_post(TD_NET_EVENT, APPLY, NULL, 0, pdMS_TO_TICKS(100));
    }
    xSemaphoreGive(s_store_lock);
    return err;
}

esp_err_t td_net_mdns_txt_set(const char *key, const char *value)
{
    if (!key || !*key || !value) return ESP_ERR_INVALID_ARG;
    if (!s_ready) return ESP_ERR_INVALID_STATE;
    return mdns_service_txt_item_set("_touchdeck", "_tcp", key, value);
}

static int stronger(const void *a, const void *b)
{
    return ((const wifi_ap_record_t *)b)->rssi - ((const wifi_ap_record_t *)a)->rssi;
}

/* Fills reply with up to SCAN_MAX access points, strongest first, and stops early when the reply would outgrow one
 * link payload. An access point whose name is not UTF-8 or is empty is left out, because JSON cannot carry it. */
static void scan_list(cJSON *reply, wifi_ap_record_t *aps, uint16_t count)
{
    cJSON *list = cJSON_AddArrayToObject(reply, "aps");
    qsort(aps, count, sizeof *aps, stronger);
    for (unsigned i = 0; list && i < count; i++) {
        const char *ssid = (const char *)aps[i].ssid;
        if (!ssid[0] || !td_net_utf8_valid(ssid)) {
            continue;
        }
        cJSON *ap = cJSON_CreateObject();
        if (!ap) {
            break;
        }
        cJSON_AddStringToObject(ap, "ssid", ssid);
        cJSON_AddNumberToObject(ap, "rssi", aps[i].rssi);
        cJSON_AddStringToObject(ap, "auth", td_net_auth_name(aps[i].authmode));
        cJSON_AddItemToArray(list, ap);
        char *json = cJSON_PrintUnformatted(reply);
        const bool fits = json && strlen(json) <= SCAN_REPLY_MAX;
        cJSON_free(json);
        if (!fits) {
            cJSON_DeleteItemFromArray(list, (int)cJSON_GetArraySize(list) - 1);
            break;
        }
    }
}

esp_err_t td_net_scan(cJSON *reply)
{
    if (!s_ready) return ESP_ERR_INVALID_STATE;
    if (xSemaphoreTake(s_scan_lock, 0) != pdTRUE) return ESP_ERR_INVALID_STATE;
    lock();
    s_scanning = true;
    unlock();
    const wifi_scan_config_t config = { .show_hidden = false, .scan_time.active = { .min = 40, .max = 100 } };
    uint16_t count = SCAN_MAX;
    wifi_ap_record_t *aps = calloc(count, sizeof *aps);
    esp_err_t err = aps ? esp_wifi_scan_start(&config, true) : ESP_ERR_NO_MEM;
    if (err == ESP_OK) err = esp_wifi_scan_get_ap_records(&count, aps);
    if (err == ESP_OK) scan_list(reply, aps, count);
    esp_wifi_clear_ap_list();
    free(aps);
    lock();
    s_scanning = false;
    unlock();
    xSemaphoreGive(s_scan_lock);
    return err;
}

static esp_err_t nvs_ready(void)
{
    esp_err_t err = nvs_flash_init();
    if (err == ESP_ERR_NVS_NO_FREE_PAGES || err == ESP_ERR_NVS_NEW_VERSION_FOUND) {
        err = nvs_flash_erase();
        if (err == ESP_OK) err = nvs_flash_init();
    }
    return err;
}

static void load_stored(void)
{
    char ssid[33] = "";
    char password[64] = "";
    size_t sn = sizeof ssid, pn = sizeof password;
    if (nvs_get_str(s_nvs, "ssid", ssid, &sn) == ESP_OK && nvs_get_str(s_nvs, "pass", password, &pn) == ESP_OK &&
        td_net_credentials_valid(ssid, password)) {
        build_config(ssid, password);
        snprintf(s_state.ssid, sizeof s_state.ssid, "%s", ssid);
        s_state.configured = true;
        s_state.state = TD_NET_CONNECTING;
    }
    memset(password, 0, sizeof password);
}

esp_err_t td_net_start(void)
{
    if (s_ready) return ESP_ERR_INVALID_STATE;
    s_lock = xSemaphoreCreateMutex();
    s_store_lock = xSemaphoreCreateMutex();
    s_scan_lock = xSemaphoreCreateMutex();
    if (!s_lock || !s_store_lock || !s_scan_lock) return ESP_ERR_NO_MEM;
    esp_err_t err = nvs_ready();
    if (err != ESP_OK) return err;
    err = nvs_open("td_net", NVS_READWRITE, &s_nvs);
    if (err != ESP_OK) return err;
    load_stored();
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
    err = mdns_service_add(s_state.host, "_touchdeck", "_tcp", TD_NET_PORT, txt, 3);
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
