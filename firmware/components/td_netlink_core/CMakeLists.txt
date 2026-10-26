idf_component_register(SRCS "td_netlink.c" "td_netlink_crypto.c" "td_netlink_srp.c" "td_netlink_record.c" "td_netlink_nvs.c"
                       INCLUDE_DIRS "include"
                       PRIV_REQUIRES td_link_core mbedtls nvs_flash)

if(CMAKE_PROJECT_NAME STREQUAL "touchdeck_test")
    target_add_binary_data(${COMPONENT_LIB}
                           "${CMAKE_CURRENT_LIST_DIR}/../../../schema/protocol/net-v1-vectors.json" TEXT)
endif()
