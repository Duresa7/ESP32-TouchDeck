#include "td_device.h"
#include "nvs.h"
#include "nvs_flash.h"
#include <stdint.h>
#include <string.h>

bool td_device_name_valid(const char *name)
{
    if (!name || !*name || strlen(name) > TD_DEVICE_NAME_MAX) return false;
    const unsigned char *p = (const unsigned char *)name;
    bool visible = false;
    while (*p) {
        uint32_t c = *p++;
        unsigned count = 0;
        uint32_t min = 0;
        if (c >= 0xc2 && c <= 0xdf) { c &= 31; count = 1; min = 0x80; }
        else if (c >= 0xe0 && c <= 0xef) { c &= 15; count = 2; min = 0x800; }
        else if (c >= 0xf0 && c <= 0xf4) { c &= 7; count = 3; min = 0x10000; }
        else if (c >= 0x80) return false;
        while (count--) {
            if ((*p & 0xc0) != 0x80) return false;
            c = (c << 6) | (*p++ & 63);
        }
        if (c < min || c > 0x10ffff || (c >= 0xd800 && c <= 0xdfff) || c < 32 || c == 127) return false;
        bool space = c == 32 || c == 0x85 || c == 0xa0 || c == 0x1680 ||
                     (c >= 0x2000 && c <= 0x200a) || c == 0x2028 || c == 0x2029 ||
                     c == 0x202f || c == 0x205f || c == 0x3000;
        visible |= !space;
    }
    return visible;
}

esp_err_t td_device_name_get(char out[TD_DEVICE_NAME_MAX + 1])
{
    strcpy(out, "Open TouchDeck");
    esp_err_t init = nvs_flash_init();
    if (init != ESP_OK) return init;
    nvs_handle_t handle;
    esp_err_t err = nvs_open("td_device", NVS_READONLY, &handle);
    if (err == ESP_ERR_NVS_NOT_FOUND) return ESP_OK;
    if (err != ESP_OK) return err;
    char value[TD_DEVICE_NAME_MAX + 1];
    size_t size = sizeof value;
    err = nvs_get_str(handle, "name", value, &size);
    nvs_close(handle);
    if (err == ESP_OK && size == strlen(value) + 1 && td_device_name_valid(value)) strcpy(out, value);
    if (err == ESP_ERR_NVS_NOT_FOUND || err == ESP_ERR_NVS_INVALID_LENGTH || err == ESP_ERR_NVS_TYPE_MISMATCH) return ESP_OK;
    return err;
}

esp_err_t td_device_name_set(const char *name)
{
    if (!td_device_name_valid(name)) return ESP_ERR_INVALID_ARG;
    char old[TD_DEVICE_NAME_MAX + 1];
    esp_err_t err = td_device_name_get(old);
    if (err != ESP_OK || strcmp(old, name) == 0) return err;
    nvs_handle_t handle;
    err = nvs_open("td_device", NVS_READWRITE, &handle);
    if (err != ESP_OK) return err;
    err = nvs_set_str(handle, "name", name);
    if (err == ESP_OK) err = nvs_commit(handle);
    nvs_close(handle);
    return err;
}
