set(td_test_glob "${CMAKE_CURRENT_LIST_DIR}/../../components/*/test/CMakeLists.txt")
# ESP-IDF reads this file once in script mode to collect requirements, and CONFIGURE_DEPENDS is invalid there.
if(CMAKE_SCRIPT_MODE_FILE)
    file(GLOB td_test_manifests "${td_test_glob}")
else()
    file(GLOB td_test_manifests CONFIGURE_DEPENDS "${td_test_glob}")
endif()

set(td_test_srcs)
set(td_test_requires)
foreach(manifest IN LISTS td_test_manifests)
    get_filename_component(test_dir "${manifest}" DIRECTORY)
    get_filename_component(component_dir "${test_dir}" DIRECTORY)
    get_filename_component(component "${component_dir}" NAME)
    set(TD_TEST_SOURCES)
    set(TD_TEST_REQUIRES)
    include("${manifest}")
    foreach(src IN LISTS TD_TEST_SOURCES)
        list(APPEND td_test_srcs "${test_dir}/${src}")
    endforeach()
    list(APPEND td_test_requires ${component} ${TD_TEST_REQUIRES})
endforeach()

idf_component_register(SRCS "test_main.c"
                       PRIV_REQUIRES unity ${td_test_requires}
                       WHOLE_ARCHIVE)
target_sources(${COMPONENT_LIB} PRIVATE ${td_test_srcs})
