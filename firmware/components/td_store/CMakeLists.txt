idf_component_register(SRCS "td_store.c" "td_store_cmds.c" "td_settings.c"
                       INCLUDE_DIRS "include"
                       REQUIRES console
                       PRIV_REQUIRES espressif__cjson joltwallet__littlefs td_console power esp_rom esp_timer freertos)
