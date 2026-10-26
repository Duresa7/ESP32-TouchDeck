idf_component_register(SRCS "td_net.c" "td_net_cmds.c" "td_net_policy.c"
                       INCLUDE_DIRS "include"
                       REQUIRES td_console espressif__cjson
                       PRIV_REQUIRES esp_wifi esp_event esp_netif esp_timer nvs_flash espressif__mdns)
