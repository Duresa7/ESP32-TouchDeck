idf_component_register(SRCS "touch.c" "touch_cmds.c"
                       INCLUDE_DIRS "include"
                       REQUIRES console
                       PRIV_REQUIRES board display power td_console td_touch_core td_ui_core esp_lcd esp_driver_gpio esp_driver_i2c esp_timer
                                     espressif__esp_lcd_touch_gt911 espressif__esp_lvgl_adapter lvgl__lvgl)
