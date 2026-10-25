set(TD_TEST_SOURCES test_td_layout.c)
set(TD_TEST_REQUIRES esp_timer)
# The test app embeds dev.json and every invalid example. builtin.json is already in td_layout itself.
file(GLOB td_layout_invalid "${CMAKE_CURRENT_LIST_DIR}/../../../../schema/examples/invalid/*.json")
set(TD_TEST_EMBED_TXTFILES "${CMAKE_CURRENT_LIST_DIR}/../../../../schema/examples/dev.json" ${td_layout_invalid})
