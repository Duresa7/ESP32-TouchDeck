#pragma once

#include "td_net.h"

#define TD_NET_NVS_NAMESPACE "td_netlink"

/* NVS must be initialised. A missing table loads as empty. */
bool td_net_nvs_load(const char *ns, td_net_apps_t *apps);
/* One blob under one key, so a commit is all or nothing. An empty table erases the key. */
bool td_net_nvs_save(const char *ns, const td_net_apps_t *apps);
