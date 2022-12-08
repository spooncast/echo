# - Try to find Libsrt
# Once done ,  this will define
#
# LIBSRT_FOUND - system has Libsrt
# LIBSRT_INCLUDE_DIRS - the Libsrt include directories
# LIBSRT_LIBRARIES - link these to use Libsrt

include(FindPackageHandleStandardArgs)

find_library(LIBSRT_LIBRARY srt
    PATHS ${LIBSRT_LIBRARYDIR})

find_path(LIBSRT_INCLUDE_DIR srt/srt.h
    PATHS ${LIBSRT_INCLUDEDIR})

find_package_handle_standard_args(srt DEFAULT_MSG
    LIBSRT_LIBRARY
    LIBSRT_INCLUDE_DIR)

mark_as_advanced(
    LIBSRT_LIBRARY
    LIBSRT_INCLUDE_DIR)

set(LIBSRT_LIBRARIES ${LIBSRT_LIBRARY})
set(LIBSRT_INCLUDE_DIRS ${LIBSRT_INCLUDE_DIR})
