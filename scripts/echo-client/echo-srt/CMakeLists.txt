add_executable(echo-srt-send
    echo-srt-send.c
    )

target_link_libraries(echo-srt-send PUBLIC
    ${LIBSRT_LIBRARIES}
    )

add_executable(echo-srt-recv
    echo-srt-recv.c
    )

target_link_libraries(echo-srt-recv PUBLIC
    ${LIBSRT_LIBRARIES}
    )
