idf_component_register(SRCS "main.c" "wifi_glyph.c"
                       PRIV_REQUIRES spi_flash esp_psram esp_timer app_update td_console td_link td_update td_net display touch ui keys power td_store espressif__cjson heap lvgl__lvgl espressif__esp_lvgl_adapter
                       INCLUDE_DIRS "")

# `idf.py build-crashing` builds with -DTD_TEST_BOOT_CRASH=1 into build-crash/, an image that aborts two seconds after
# app_main so the rollback can be checked on the board.
if(TD_TEST_BOOT_CRASH)
    target_compile_definitions(${COMPONENT_LIB} PRIVATE TD_TEST_BOOT_CRASH=1)
endif()
