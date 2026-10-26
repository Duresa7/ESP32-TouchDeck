idf_component_register(SRCS "td_net.c" "td_net_crypto.c" "td_net_srp.c" "td_net_record.c" "td_net_nvs.c"
                       INCLUDE_DIRS "include"
                       PRIV_REQUIRES td_link_core mbedtls nvs_flash)

if(CMAKE_PROJECT_NAME STREQUAL "touchdeck_test")
    target_add_binary_data(${COMPONENT_LIB}
                           "${CMAKE_CURRENT_LIST_DIR}/../../../schema/protocol/net-v1-vectors.json" TEXT)
endif()
