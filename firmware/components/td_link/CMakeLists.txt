idf_component_register(SRCS "td_link.c" "td_link_msgs.c" "td_link_cmds.c"
                       INCLUDE_DIRS "include"
                       REQUIRES td_console espressif__cjson
                       PRIV_REQUIRES td_link_core td_store esp_timer esp_app_format app_update)
