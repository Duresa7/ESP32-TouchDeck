#include "td_link.h"
#include "td_device.h"
#include "unity.h"
#include <stdlib.h>
#include <string.h>

TEST_CASE("v2 hello fits a frame with the maximum escaped name and version", "[td_link]")
{
    char old[TD_DEVICE_NAME_MAX + 1];
    TEST_ASSERT_EQUAL(ESP_OK, td_device_name_get(old));
    char name[TD_DEVICE_NAME_MAX + 1];
    memset(name, '"', TD_DEVICE_NAME_MAX);
    name[TD_DEVICE_NAME_MAX] = 0;
    TEST_ASSERT_EQUAL(ESP_OK, td_device_name_set(name));
    cJSON *request = cJSON_Parse("{\"m\":\"hello\",\"proto\":2}");
    cJSON *reply = td_link_dispatch(request);
    TEST_ASSERT_EQUAL(ESP_OK, td_device_name_set(old));
    TEST_ASSERT_NOT_NULL(reply);
    TEST_ASSERT_TRUE(cJSON_IsTrue(cJSON_GetObjectItemCaseSensitive(reply, "ok")));
    TEST_ASSERT_EQUAL_STRING(name, cJSON_GetStringValue(cJSON_GetObjectItemCaseSensitive(reply, "name")));
    cJSON_ReplaceItemInObjectCaseSensitive(reply, "version", cJSON_CreateString("12345678901234567890123456789012"));
    char *text = cJSON_PrintUnformatted(reply);
    TEST_ASSERT_NOT_NULL(text);
    TEST_ASSERT_LESS_OR_EQUAL_UINT(1024, strlen(text));
    free(text);
    cJSON_Delete(reply);
    cJSON_Delete(request);
}

TEST_CASE("Device name persists through repeated NVS opens and refused writes", "[td_link]")
{
    char old[49], stored[49];
    TEST_ASSERT_EQUAL(ESP_OK, td_device_name_get(old));
    TEST_ASSERT_EQUAL(ESP_OK, td_device_name_set("Test Device"));
    esp_err_t rejected = td_device_name_set("\n");
    esp_err_t read = td_device_name_get(stored);
    TEST_ASSERT_EQUAL(ESP_OK, td_device_name_set(old));
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_ARG, rejected);
    TEST_ASSERT_EQUAL(ESP_OK, read);
    TEST_ASSERT_EQUAL_STRING("Test Device", stored);
}

TEST_CASE("idle settings persist through NVS and an out-of-range set changes nothing", "[td_link]")
{
    td_idle_settings_t old, stored;
    TEST_ASSERT_EQUAL(ESP_OK, td_device_idle_get(&old));
    const td_idle_settings_t next = { .enabled = true, .timeout_s = 120, .interval_s = 30 };
    const td_idle_settings_t outside = { .enabled = true, .timeout_s = 9, .interval_s = 30 };
    TEST_ASSERT_EQUAL(ESP_OK, td_device_idle_set(&next));
    esp_err_t rejected = td_device_idle_set(&outside);
    esp_err_t read = td_device_idle_get(&stored);
    TEST_ASSERT_EQUAL(ESP_OK, td_device_idle_set(&old));
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_ARG, rejected);
    TEST_ASSERT_EQUAL(ESP_OK, read);
    TEST_ASSERT_TRUE(stored.enabled);
    TEST_ASSERT_EQUAL_UINT32(120, stored.timeout_s);
    TEST_ASSERT_EQUAL_UINT32(30, stored.interval_s);
}
