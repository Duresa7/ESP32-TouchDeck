idf_component_register(SRCS "power.c" "power_cmds.c"
                       INCLUDE_DIRS "include"
                       REQUIRES console td_touch_core
                       PRIV_REQUIRES board keys td_console esp_timer freertos)
