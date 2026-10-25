idf_component_register(SRCS "key_engine.c" "key_sink_log.c" "keys.c" "keys_console.c"
                       INCLUDE_DIRS "include"
                       REQUIRES console
                       PRIV_REQUIRES td_console esp_timer)
