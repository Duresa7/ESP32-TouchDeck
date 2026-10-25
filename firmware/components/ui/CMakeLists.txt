idf_component_register(SRCS "ui_theme.c" "ui_deck.c" "ui_cmds.c"
                       INCLUDE_DIRS "include"
                       REQUIRES console
                       PRIV_REQUIRES board display touch td_console td_ui_core
                                     espressif__esp_lvgl_adapter lvgl__lvgl)
