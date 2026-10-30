set(TD_TEST_SOURCES test_ui_pager.c test_ui_pages.c test_fkey_page.c test_media_page.c test_ui_widget_state.c
                    test_ui_geometry.c test_ui_redraw.c test_ui_live.c test_ui_graph.c)
set(TD_TEST_REQUIRES keys td_layout espressif__cjson)
set(TD_TEST_EMBED_TXTFILES "${CMAKE_CURRENT_LIST_DIR}/../../../../schema/live/graph-vectors.json")
