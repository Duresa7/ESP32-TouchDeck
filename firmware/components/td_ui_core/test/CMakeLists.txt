set(TD_TEST_SOURCES test_ui_pager.c test_ui_pages.c test_fkey_page.c test_media_page.c test_ui_widget_state.c
                    test_ui_geometry.c test_ui_redraw.c test_ui_live.c test_ui_live_media.c test_ui_graph.c
                    test_ui_bar.c test_ui_now_playing.c)
set(TD_TEST_REQUIRES keys td_layout espressif__cjson)
set(TD_TEST_EMBED_TXTFILES "${CMAKE_CURRENT_LIST_DIR}/../../../../schema/live/graph-vectors.json"
                           "${CMAKE_CURRENT_LIST_DIR}/../../../../schema/live/bar-vectors.json"
                           "${CMAKE_CURRENT_LIST_DIR}/../../../../schema/live/now-playing-vectors.json"
                           "${CMAKE_CURRENT_LIST_DIR}/../../../../schema/live/text-vectors.json")
