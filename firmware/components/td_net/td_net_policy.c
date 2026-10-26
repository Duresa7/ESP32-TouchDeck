#include "td_net.h"
#include <string.h>

bool td_net_credentials_valid(const char *ssid, const char *password)
{
    if (!ssid || !password) {
        return false;
    }
    const size_t sn = strlen(ssid);
    const size_t pn = strlen(password);
    return sn > 0 && sn <= 32 && (pn == 0 || (pn >= 8 && pn <= 63));
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
