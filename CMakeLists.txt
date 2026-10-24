# ESP-IDF project root. Firmware sources live in main/.
cmake_minimum_required(VERSION 3.22)

include($ENV{IDF_PATH}/tools/cmake/project.cmake)
# Only build components that main/ (and its dependencies) ask for.
idf_build_set_property(MINIMAL_BUILD ON)
project(touchdeck)
