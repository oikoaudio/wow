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
    GIT_TAG 1cca996e96f29ab2be7ae9f8cfe532bbc92e1dd6 # v0.16.0
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

# With Makefile generators, clap-wrapper's PRE_BUILD plist copy can run before
# CMake writes its default bundle plist, leaving a valid-looking component with
# no AudioComponents registration. Finalize the metadata on every package build.
set(WOW_AUV2_INFO_PLIST
    "${CMAKE_CURRENT_BINARY_DIR}/oiko_wow_auv2-build-helper-output/auv2_Info.plist"
)
set(WOW_AUV2_EMBEDDED_CLAP
    "$<TARGET_BUNDLE_DIR:oiko_wow_auv2>/Contents/PlugIns/Oiko Wow.clap"
)
add_custom_command(
    TARGET oiko_wow_auv2
    POST_BUILD
    COMMAND "${CMAKE_COMMAND}" -E copy
        "${WOW_AUV2_INFO_PLIST}"
        "$<TARGET_BUNDLE_DIR:oiko_wow_auv2>/Contents/Info.plist"
    COMMENT "Installing Oiko Wow Audio Unit registration metadata"
)
add_custom_target(
    oiko_wow_auv2_finalize
    COMMAND "${CMAKE_COMMAND}" -E rm -rf
        "${WOW_AUV2_EMBEDDED_CLAP}"
    COMMAND "${CMAKE_COMMAND}" -E copy_directory
        "${WOW_CLAP_BUNDLE}"
        "${WOW_AUV2_EMBEDDED_CLAP}"
    COMMAND "${CMAKE_COMMAND}" -E copy
        "${WOW_AUV2_INFO_PLIST}"
        "$<TARGET_BUNDLE_DIR:oiko_wow_auv2>/Contents/Info.plist"
    DEPENDS oiko_wow_auv2
    COMMENT "Refreshing embedded CLAP and finalizing Oiko Wow Audio Unit"
)
