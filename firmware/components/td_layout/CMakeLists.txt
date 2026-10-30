idf_component_register(SRCS "td_layout.c" "td_layout_parse.c"
                       INCLUDE_DIRS "include"
                       REQUIRES td_ui_core
                       PRIV_REQUIRES espressif__cjson heap td_update
                       EMBED_TXTFILES "${CMAKE_CURRENT_LIST_DIR}/../../../schema/examples/builtin.json")
