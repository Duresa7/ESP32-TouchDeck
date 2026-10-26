idf_component_register(SRCS "main.c"
                       PRIV_REQUIRES spi_flash esp_psram esp_timer app_update td_console td_link display touch ui keys power td_store espressif__cjson heap
                       INCLUDE_DIRS "")
