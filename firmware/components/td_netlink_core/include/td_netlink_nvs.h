#pragma once

#include "td_netlink.h"

#define TD_NETLINK_NVS_NAMESPACE "td_netlink"

/* NVS must be initialised. A missing table loads as empty. */
bool td_netlink_nvs_load(const char *ns, td_netlink_apps_t *apps);
/* One blob under one key, so a commit is all or nothing. An empty table erases the key. */
bool td_netlink_nvs_save(const char *ns, const td_netlink_apps_t *apps);
