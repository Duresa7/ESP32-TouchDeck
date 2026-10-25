idf_component_register(SRCS "td_console.c" "td_cmd_table.c" "td_reply.c" "td_rx_sync.c"
                       INCLUDE_DIRS "include"
                       REQUIRES console
                       PRIV_REQUIRES esp_app_format esp_driver_usb_serial_jtag esp_timer)
