idf_component_register(SRCS "td_console.c" "td_reply.c"
                       INCLUDE_DIRS "include"
                       PRIV_REQUIRES console esp_app_format esp_driver_usb_serial_jtag esp_timer)
