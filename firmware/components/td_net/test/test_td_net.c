#include "td_net.h"
#include "td_net_priv.h"
#include <string.h>
#include "esp_wifi_types.h"
#include "unity.h"

TEST_CASE("wifi credentials enforce byte limits and allow open networks", "[td_net]")
{
    TEST_ASSERT_TRUE(td_net_credentials_valid("deck", ""));
    TEST_ASSERT_TRUE(td_net_credentials_valid("deck", "12345678"));
    TEST_ASSERT_FALSE(td_net_credentials_valid("", "12345678"));
    TEST_ASSERT_FALSE(td_net_credentials_valid("deck", "1234567"));
    TEST_ASSERT_FALSE(td_net_credentials_valid(NULL, "12345678"));
    TEST_ASSERT_FALSE(td_net_credentials_valid("deck", NULL));
    char ssid[34], pass[65];
    memset(ssid, 's', 33);
    memset(pass, 'p', 64);
    ssid[32] = '\0';
    pass[63] = '\0';
    TEST_ASSERT_TRUE(td_net_credentials_valid(ssid, pass));
    ssid[32] = 's'; ssid[33] = '\0';
    TEST_ASSERT_FALSE(td_net_credentials_valid(ssid, pass));
    ssid[32] = '\0'; pass[63] = 'p'; pass[64] = '\0';
    TEST_ASSERT_FALSE(td_net_credentials_valid(ssid, pass));
}

TEST_CASE("wifi ssid must be well formed utf-8 within 32 bytes", "[td_net]")
{
    TEST_ASSERT_TRUE(td_net_credentials_valid("caf\xc3\xa9 \xe2\x82\xac \xf0\x9f\x93\xb6", ""));
    TEST_ASSERT_FALSE(td_net_credentials_valid("bad\xff", ""));
    TEST_ASSERT_FALSE(td_net_credentials_valid("cut\xc3", ""));
    TEST_ASSERT_FALSE(td_net_credentials_valid("over\xc0\x80long", ""));
    TEST_ASSERT_FALSE(td_net_credentials_valid("sur\xed\xa0\x80rogate", ""));
    TEST_ASSERT_FALSE(td_net_credentials_valid("big\xf4\x90\x80\x80", ""));
    TEST_ASSERT_FALSE(td_net_credentials_valid("lone\x80", ""));
    char wide[40];
    memset(wide, 0, sizeof wide);
    for (int i = 0; i < 11; i++) memcpy(wide + 3 * i, "\xe2\x82\xac", 3);
    TEST_ASSERT_FALSE(td_net_credentials_valid(wide, ""));
    wide[30] = '\0';
    TEST_ASSERT_TRUE(td_net_credentials_valid(wide, ""));
}

TEST_CASE("wifi reconnect backoff reaches thirty seconds and stays there", "[td_net]")
{
    TEST_ASSERT_EQUAL_UINT(1, td_net_retry_seconds(0));
    TEST_ASSERT_EQUAL_UINT(2, td_net_retry_seconds(1));
    TEST_ASSERT_EQUAL_UINT(4, td_net_retry_seconds(2));
    TEST_ASSERT_EQUAL_UINT(8, td_net_retry_seconds(3));
    TEST_ASSERT_EQUAL_UINT(30, td_net_retry_seconds(4));
    TEST_ASSERT_EQUAL_UINT(30, td_net_retry_seconds(100000));
}

TEST_CASE("wifi disconnect reasons collapse to the words the link reports", "[td_net]")
{
    TEST_ASSERT_EQUAL_STRING("no_ap", td_net_reason_word(WIFI_REASON_NO_AP_FOUND));
    TEST_ASSERT_EQUAL_STRING("no_ap", td_net_reason_word(WIFI_REASON_NO_AP_FOUND_IN_AUTHMODE_THRESHOLD));
    TEST_ASSERT_EQUAL_STRING("auth", td_net_reason_word(WIFI_REASON_AUTH_FAIL));
    TEST_ASSERT_EQUAL_STRING("auth", td_net_reason_word(WIFI_REASON_4WAY_HANDSHAKE_TIMEOUT));
    TEST_ASSERT_EQUAL_STRING("assoc", td_net_reason_word(WIFI_REASON_ASSOC_FAIL));
    TEST_ASSERT_EQUAL_STRING("beacon", td_net_reason_word(WIFI_REASON_BEACON_TIMEOUT));
    TEST_ASSERT_EQUAL_STRING("left", td_net_reason_word(WIFI_REASON_ASSOC_LEAVE));
    TEST_ASSERT_EQUAL_STRING("disconnected", td_net_reason_word(WIFI_REASON_UNSPECIFIED));
    TEST_ASSERT_EQUAL_STRING("disconnected", td_net_reason_word(9999));
}

TEST_CASE("wifi scan names every security mode as a word", "[td_net]")
{
    TEST_ASSERT_EQUAL_STRING("open", td_net_auth_name(WIFI_AUTH_OPEN));
    TEST_ASSERT_EQUAL_STRING("wep", td_net_auth_name(WIFI_AUTH_WEP));
    TEST_ASSERT_EQUAL_STRING("wpa", td_net_auth_name(WIFI_AUTH_WPA_PSK));
    TEST_ASSERT_EQUAL_STRING("wpa2", td_net_auth_name(WIFI_AUTH_WPA2_PSK));
    TEST_ASSERT_EQUAL_STRING("wpa_wpa2", td_net_auth_name(WIFI_AUTH_WPA_WPA2_PSK));
    TEST_ASSERT_EQUAL_STRING("wpa3", td_net_auth_name(WIFI_AUTH_WPA3_PSK));
    TEST_ASSERT_EQUAL_STRING("wpa2_wpa3", td_net_auth_name(WIFI_AUTH_WPA2_WPA3_PSK));
    TEST_ASSERT_EQUAL_STRING("owe", td_net_auth_name(WIFI_AUTH_OWE));
    TEST_ASSERT_EQUAL_STRING("enterprise", td_net_auth_name(WIFI_AUTH_WPA2_ENTERPRISE));
    TEST_ASSERT_EQUAL_STRING("unknown", td_net_auth_name(WIFI_AUTH_UNKNOWN));
    TEST_ASSERT_EQUAL_STRING("unknown", td_net_auth_name(9999));
}

TEST_CASE("wifi state names are the four words the link reports", "[td_net]")
{
    TEST_ASSERT_EQUAL_STRING("off", td_net_status_name(TD_NET_OFF));
    TEST_ASSERT_EQUAL_STRING("connecting", td_net_status_name(TD_NET_CONNECTING));
    TEST_ASSERT_EQUAL_STRING("connected", td_net_status_name(TD_NET_CONNECTED));
    TEST_ASSERT_EQUAL_STRING("failed", td_net_status_name(TD_NET_FAILED));
    TEST_ASSERT_EQUAL_STRING("failed", td_net_status_name((td_net_status_t)77));
}

TEST_CASE("wifi state before start is off with no network", "[td_net]")
{
    const td_net_state_t s = td_net_state();
    TEST_ASSERT_FALSE(s.configured);
    TEST_ASSERT_EQUAL(TD_NET_OFF, s.state);
    TEST_ASSERT_EQUAL(TD_NET_OFF, td_net_status());
    TEST_ASSERT_EQUAL_STRING("", s.ip);
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_STATE, td_net_set("deck", "12345678"));
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_ARG, td_net_set("deck", "short"));
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_STATE, td_net_forget());
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_STATE, td_net_mdns_txt_set("paired", "0"));
}
