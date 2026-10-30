set(TD_TEST_SOURCES test_td_identity.c)
set(TD_TEST_REQUIRES td_store)
set(TD_TEST_EMBED_TXTFILES "${CMAKE_CURRENT_LIST_DIR}/../../../../schema/protocol/golden-hello.json")
