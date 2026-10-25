idf_component_register(SRCS "display.c" "display_cmds.c" "panel_rgb.c"
                       INCLUDE_DIRS "include"
                       REQUIRES console
                       PRIV_REQUIRES board td_shot td_console esp_lcd esp_timer
                                     espressif__esp_lvgl_adapter lvgl__lvgl)
