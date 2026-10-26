#include "td_net_nvs.h"
#include "nvs.h"
#include <string.h>

#define BLOB_KEY "apps"
#define BLOB_VERSION 1
#define ENTRY_LEN (TD_NET_APP_ID_LEN + 1 + TD_NET_NAME_MAX + TD_NET_KEY_LEN)

typedef struct {
    uint8_t version;
    uint8_t count;
    uint8_t entries[TD_NET_APPS_MAX][ENTRY_LEN];
} blob_t;

bool td_net_nvs_load(const char *ns, td_net_apps_t *apps)
{
    memset(apps, 0, sizeof *apps);
    nvs_handle_t handle;
    esp_err_t err = nvs_open(ns, NVS_READONLY, &handle);
    if (err == ESP_ERR_NVS_NOT_FOUND) {
        return true;
    }
    if (err != ESP_OK) {
        return false;
    }
    blob_t blob;
    size_t len = sizeof blob;
    err = nvs_get_blob(handle, BLOB_KEY, &blob, &len);
    nvs_close(handle);
    if (err == ESP_ERR_NVS_NOT_FOUND) {
        return true;
    }
    bool ok = err == ESP_OK && blob.version == BLOB_VERSION && blob.count <= TD_NET_APPS_MAX
              && len == 2 + (size_t)blob.count * ENTRY_LEN;
    for (uint8_t i = 0; ok && i < blob.count; i++) {
        td_net_app_t *app = &apps->apps[i];
        const uint8_t *e = blob.entries[i];
        memcpy(app->app_id, e, TD_NET_APP_ID_LEN);
        app->name_len = e[TD_NET_APP_ID_LEN];
        memcpy(app->name, e + TD_NET_APP_ID_LEN + 1, TD_NET_NAME_MAX);
        memcpy(app->ltk, e + TD_NET_APP_ID_LEN + 1 + TD_NET_NAME_MAX, TD_NET_KEY_LEN);
        ok = app->name_len >= 1 && app->name_len <= TD_NET_NAME_MAX;
    }
    apps->count = ok ? blob.count : 0;
    if (!ok) {
        td_net_wipe(apps, sizeof *apps);
    }
    td_net_wipe(&blob, sizeof blob);
    return ok;
}

bool td_net_nvs_save(const char *ns, const td_net_apps_t *apps)
{
    nvs_handle_t handle;
    if (nvs_open(ns, NVS_READWRITE, &handle) != ESP_OK) {
        return false;
    }
    esp_err_t err;
    if (apps->count == 0) {
        err = nvs_erase_key(handle, BLOB_KEY);
        if (err == ESP_ERR_NVS_NOT_FOUND) {
            err = ESP_OK;
        }
    } else {
        blob_t blob = {.version = BLOB_VERSION, .count = apps->count};
        for (uint8_t i = 0; i < apps->count; i++) {
            const td_net_app_t *app = &apps->apps[i];
            uint8_t *e = blob.entries[i];
            memcpy(e, app->app_id, TD_NET_APP_ID_LEN);
            e[TD_NET_APP_ID_LEN] = app->name_len;
            memcpy(e + TD_NET_APP_ID_LEN + 1, app->name, TD_NET_NAME_MAX);
            memcpy(e + TD_NET_APP_ID_LEN + 1 + TD_NET_NAME_MAX, app->ltk, TD_NET_KEY_LEN);
        }
        err = nvs_set_blob(handle, BLOB_KEY, &blob, 2 + (size_t)apps->count * ENTRY_LEN);
        td_net_wipe(&blob, sizeof blob);
    }
    if (err == ESP_OK) {
        err = nvs_commit(handle);
    }
    nvs_close(handle);
    return err == ESP_OK;
}
