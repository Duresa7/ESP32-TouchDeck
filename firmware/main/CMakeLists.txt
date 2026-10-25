idf_component_register(SRCS "main.c"
                       PRIV_REQUIRES spi_flash esp_psram esp_timer app_update td_console display touch ui keys power
                       INCLUDE_DIRS "")
