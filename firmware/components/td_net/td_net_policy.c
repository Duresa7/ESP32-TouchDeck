#include "td_net.h"
#include <string.h>
#include "esp_wifi_types.h"

bool td_net_utf8_valid(const char *s)
{
    const unsigned char *p = (const unsigned char *)s;
    while (*p) {
        if (*p < 0x80) {
            p++;
            continue;
        }
        size_t extra;
        uint32_t cp, least;
        if ((*p & 0xe0) == 0xc0) {
            extra = 1, cp = *p & 0x1f, least = 0x80;
        } else if ((*p & 0xf0) == 0xe0) {
            extra = 2, cp = *p & 0x0f, least = 0x800;
        } else if ((*p & 0xf8) == 0xf0) {
            extra = 3, cp = *p & 0x07, least = 0x10000;
        } else {
            return false;
        }
        for (size_t i = 1; i <= extra; i++) {
            if ((p[i] & 0xc0) != 0x80) {
                return false;
            }
            cp = (cp << 6) | (p[i] & 0x3f);
        }
        if (cp < least || cp > 0x10ffff || (cp >= 0xd800 && cp <= 0xdfff)) {
            return false;
        }
        p += extra + 1;
    }
    return true;
}

bool td_net_credentials_valid(const char *ssid, const char *password)
{
    if (!ssid || !password) {
        return false;
    }
    const size_t sn = strlen(ssid);
    const size_t pn = strlen(password);
    return sn > 0 && sn <= 32 && td_net_utf8_valid(ssid) && (pn == 0 || (pn >= 8 && pn <= 63));
}

unsigned td_net_retry_seconds(unsigned attempt)
{
    static const unsigned delays[] = { 1, 2, 4, 8, 30 };
    return delays[attempt < 4 ? attempt : 4];
}

const char *td_net_status_name(td_net_status_t state)
{
    static const char *const names[] = { "off", "connecting", "connected", "failed" };
    return state <= TD_NET_FAILED && state >= TD_NET_OFF ? names[state] : "failed";
}

const char *td_net_reason_word(unsigned reason)
{
    switch (reason) {
    case WIFI_REASON_NO_AP_FOUND:
    case WIFI_REASON_NO_AP_FOUND_W_COMPATIBLE_SECURITY:
    case WIFI_REASON_NO_AP_FOUND_IN_AUTHMODE_THRESHOLD:
    case WIFI_REASON_NO_AP_FOUND_IN_RSSI_THRESHOLD:
        return "no_ap";
    case WIFI_REASON_AUTH_FAIL:
    case WIFI_REASON_AUTH_EXPIRE:
    case WIFI_REASON_HANDSHAKE_TIMEOUT:
    case WIFI_REASON_4WAY_HANDSHAKE_TIMEOUT:
    case WIFI_REASON_MIC_FAILURE:
    case WIFI_REASON_802_1X_AUTH_FAILED:
        return "auth";
    case WIFI_REASON_ASSOC_FAIL:
    case WIFI_REASON_ASSOC_TOOMANY:
    case WIFI_REASON_CONNECTION_FAIL:
        return "assoc";
    case WIFI_REASON_BEACON_TIMEOUT:
    case WIFI_REASON_AP_TSF_RESET:
        return "beacon";
    case WIFI_REASON_AUTH_LEAVE:
    case WIFI_REASON_ASSOC_LEAVE:
    case WIFI_REASON_DISASSOC_DUE_TO_INACTIVITY:
        return "left";
    default:
        return "disconnected";
    }
}

const char *td_net_auth_name(unsigned auth)
{
    switch (auth) {
    case WIFI_AUTH_OPEN: return "open";
    case WIFI_AUTH_WEP: return "wep";
    case WIFI_AUTH_WPA_PSK: return "wpa";
    case WIFI_AUTH_WPA2_PSK: return "wpa2";
    case WIFI_AUTH_WPA_WPA2_PSK: return "wpa_wpa2";
    case WIFI_AUTH_WPA3_PSK: return "wpa3";
    case WIFI_AUTH_WPA2_WPA3_PSK: return "wpa2_wpa3";
    case WIFI_AUTH_OWE: return "owe";
    case WIFI_AUTH_ENTERPRISE:
    case WIFI_AUTH_WPA3_ENT_192:
    case WIFI_AUTH_WPA3_ENTERPRISE:
    case WIFI_AUTH_WPA2_WPA3_ENTERPRISE:
    case WIFI_AUTH_WPA_ENTERPRISE:
        return "enterprise";
    default: return "unknown";
    }
}
