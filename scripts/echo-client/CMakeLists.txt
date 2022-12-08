cmake_minimum_required(VERSION 2.8 FATAL_ERROR)
set(PROJECT_NAME_STR echo-pseudo-client)
project(${PROJECT_NAME_STR} C)

set(CMAKE_POSITION_INDEPENDENT_CODE ON)
set(CMAKE_MODULE_PATH ${CMAKE_CURRENT_SOURCE_DIR}/cmake ${CMAKE_MODULE_PATH})

find_package(Srt REQUIRED)

include_directories(
    ${CMAKE_SOURCE_DIR}
    ${LIBSRT_INCLUDE_DIRS}
    )

add_subdirectory("echo-srt")
