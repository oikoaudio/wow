cmake_minimum_required(VERSION 3.21)

project(oiko-wow-auv2 VERSION 0.1.1 LANGUAGES C CXX OBJC OBJCXX)

if(NOT APPLE)
    message(FATAL_ERROR "The Oiko Wow AUv2 wrapper can only be built on macOS")
endif()

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

set(WOW_CLAP_BUNDLE "" CACHE PATH "Path to the universal Oiko Wow.clap bundle")
if(NOT IS_DIRECTORY "${WOW_CLAP_BUNDLE}")
    message(FATAL_ERROR "WOW_CLAP_BUNDLE must point to the Oiko Wow.clap bundle")
endif()

include(FetchContent)
set(CLAP_WRAPPER_DOWNLOAD_DEPENDENCIES TRUE CACHE BOOL "" FORCE)
set(CLAP_WRAPPER_BUILD_TESTS FALSE CACHE BOOL "" FORCE)
set(CLAP_WRAPPER_COPY_AFTER_BUILD FALSE CACHE BOOL "" FORCE)
FetchContent_Declare(
    clap_wrapper
    GIT_REPOSITORY https://github.com/free-audio/clap-wrapper.git
    GIT_TAG 35f524b771ec09f54c164720bb90f271273b37d3
)
FetchContent_MakeAvailable(clap_wrapper)

add_library(oiko_wow_auv2 MODULE)
target_add_auv2_wrapper(
    TARGET oiko_wow_auv2
    OUTPUT_NAME "Oiko Wow"
    BUNDLE_IDENTIFIER "com.oikoaudio.wow.auv2"
    BUNDLE_VERSION "${PROJECT_VERSION}"
    PREFER_CMAKE_AUV2_CONFIGURATION TRUE
    MANUFACTURER_NAME "Oiko Audio"
    MANUFACTURER_CODE "Oiko"
    SUBTYPE_CODE "OWow"
    INSTRUMENT_TYPE "aufx"
    MACOS_EMBEDDED_CLAP_LOCATION "${WOW_CLAP_BUNDLE}"
)
