idf_component_register(SRCS "td_update_state.c" "td_update.c" "td_update_cmds.c"
                       INCLUDE_DIRS "include"
                       REQUIRES console
                       PRIV_REQUIRES td_console app_update esp_app_format esp_partition esp_system esp_timer)
