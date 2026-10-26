idf_component_register(SRCS "td_netlink_service.c" "td_netlink_cmds.c"
                       INCLUDE_DIRS "include"
                       REQUIRES td_console espressif__cjson
                       PRIV_REQUIRES td_netlink_core ui lwip esp_netif esp_hw_support esp_timer freertos nvs_flash log)
