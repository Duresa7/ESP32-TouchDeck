idf_component_register(SRCS "touch.c"
                       INCLUDE_DIRS "include"
                       REQUIRES console
                       PRIV_REQUIRES board td_touch_core esp_lcd esp_driver_gpio esp_driver_i2c esp_timer
                                     espressif__esp_lcd_touch_gt911 espressif__esp_lvgl_adapter lvgl__lvgl)
