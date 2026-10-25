idf_component_register(SRCS "td_link.c" "td_link_msgs.c" "td_link_cmds.c" "td_link_files.c" "key_sink_link.c"
                       INCLUDE_DIRS "include"
                       REQUIRES td_console espressif__cjson
                       PRIV_REQUIRES td_link_core keys td_layout td_store ui power esp_timer esp_app_format app_update heap)
