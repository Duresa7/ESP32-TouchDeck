idf_component_register(SRCS "display.c" "display_cmds.c" "panel_rgb.c" "display_rotate.c"
                       INCLUDE_DIRS "include"
                       REQUIRES console
                       PRIV_REQUIRES board td_shot td_console td_rotate esp_lcd esp_timer
                                     espressif__esp_lvgl_adapter lvgl__lvgl)

# display_rotate.c replaces esp_lvgl_adapter's software rotation copy by wrapping the symbol at link time, so the
# managed component stays as the registry ships it. The wrapper relies on that function's arguments and mapping in
# the pinned 0.7.3, so stop the build if another version arrives until someone rechecks it.
set(td_adapter_version "0.7.3")
idf_component_get_property(td_adapter_dir espressif__esp_lvgl_adapter COMPONENT_DIR)
file(STRINGS "${td_adapter_dir}/idf_component.yml" td_adapter_version_line REGEX "^version:")
if(NOT td_adapter_version_line MATCHES "^version: *['\"]?${td_adapter_version}['\"]?$")
    message(FATAL_ERROR "display_rotate.c wraps esp_lvgl_adapter ${td_adapter_version}'s display_rotate_copy_region, "
                        "but the build has '${td_adapter_version_line}'. Recheck the wrapper against it first.")
endif()
target_link_libraries(${COMPONENT_LIB} INTERFACE "-Wl,--wrap=display_rotate_copy_region"
                                                 "-u __wrap_display_rotate_copy_region")
