cmake_minimum_required(VERSION 3.15)
project(wickra_c_examples C CXX)

# Directory holding the compiled Wickra C library (cargo output), e.g.
# <workspace>/target/release. Override with -DWICKRA_LIB_DIR=/path/to/target/release.
if(NOT DEFINED WICKRA_LIB_DIR)
    set(WICKRA_LIB_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../target/release")
endif()
set(WICKRA_INCLUDE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../bindings/c/include")

# Absolute path to the bundled OHLCV datasets, baked into the data-driven
# examples as a compile definition so they run from any working directory (the
# C counterpart of the Rust examples' CARGO_MANIFEST_DIR).
get_filename_component(WICKRA_DATA_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../data" ABSOLUTE)

# OpenMP is optional: parallel_assets links it when present and falls back to a
# single-threaded run otherwise.
find_package(OpenMP QUIET)

# Pick the right link target per platform/toolchain.
#  - MSVC links the generated import library (wickra.dll.lib).
#  - MinGW/gcc on Windows links the DLL directly.
#  - Unix links the shared object / dylib.
if(WIN32)
    set(WICKRA_RUNTIME "${WICKRA_LIB_DIR}/wickra.dll")
    if(MSVC)
        set(WICKRA_LINK_LIB "${WICKRA_LIB_DIR}/wickra.dll.lib")
    else()
        set(WICKRA_LINK_LIB "${WICKRA_LIB_DIR}/wickra.dll")
    endif()
elseif(APPLE)
    set(WICKRA_LINK_LIB "${WICKRA_LIB_DIR}/libwickra.dylib")
else()
    set(WICKRA_LINK_LIB "${WICKRA_LIB_DIR}/libwickra.so")
endif()

enable_testing()

# Build one example and link it to the Wickra library. On Windows the DLL is
# copied next to the executable so the loader finds it at run time. With
# register_test=TRUE the example is also run as a ctest; network examples pass
# FALSE (they are built only, never run in CI).
function(add_wickra_example name source register_test)
    add_executable(${name} ${source})
    target_include_directories(${name} PRIVATE "${WICKRA_INCLUDE_DIR}")
    target_link_libraries(${name} PRIVATE "${WICKRA_LINK_LIB}")
    target_compile_definitions(${name} PRIVATE "WICKRA_DATA_DIR=\"${WICKRA_DATA_DIR}\"")
    if(UNIX AND NOT APPLE)
        target_link_libraries(${name} PRIVATE m)
    endif()
    if(WIN32)
        add_custom_command(TARGET ${name} POST_BUILD
            COMMAND ${CMAKE_COMMAND} -E copy_if_different
                    "${WICKRA_RUNTIME}" "$<TARGET_FILE_DIR:${name}>")
    endif()
    if(register_test)
        add_test(NAME ${name} COMMAND ${name})
        if(NOT WIN32)
            set_tests_properties(${name} PROPERTIES
                ENVIRONMENT "LD_LIBRARY_PATH=${WICKRA_LIB_DIR};DYLD_LIBRARY_PATH=${WICKRA_LIB_DIR}")
        endif()
    endif()
endfunction()

# Offline examples — built and run as ctests.
add_wickra_example(smoke smoke.c TRUE)                 # links the boundary, asserts values
add_wickra_example(archetypes archetypes.c TRUE)       # one indicator per FFI archetype
add_wickra_example(streaming streaming.c TRUE)         # multi-indicator tick stream
add_wickra_example(cpp_smoke smoke.cpp TRUE)           # C++ RAII wrapper (wickra.hpp)
add_wickra_example(streaming_vs_batch_test streaming_vs_batch_test.c TRUE)  # update() == batch()
add_wickra_example(backtest backtest.c TRUE)           # indicator basket over a CSV
add_wickra_example(multi_timeframe multi_timeframe.c TRUE)  # resample + per-TF indicators
add_wickra_example(parallel_assets parallel_assets.c TRUE)  # serial vs OpenMP fan-out
add_wickra_example(strategy_rsi_mean_reversion strategy_rsi_mean_reversion.c TRUE)
add_wickra_example(strategy_macd_adx strategy_macd_adx.c TRUE)
add_wickra_example(strategy_bollinger_squeeze strategy_bollinger_squeeze.c TRUE)

if(OpenMP_C_FOUND)
    target_link_libraries(parallel_assets PRIVATE OpenMP::OpenMP_C)
endif()

# Golden-fixture parity over the whole 515-indicator catalogue, built and run as
# BOTH C (golden_test.c) and C++ (golden_test.cpp #includes the same source) to
# prove the C ABI header is consumable from each language. The fixtures live at
# the repo root; pass the absolute path as the test argument.
get_filename_component(WICKRA_GOLDEN_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../testdata/golden" ABSOLUTE)
foreach(gt_pair "golden_test:golden_test.c" "golden_test_cpp:golden_test.cpp"
                "data_layer_test:data_layer_test.c" "data_layer_test_cpp:data_layer_test.cpp")
    string(REPLACE ":" ";" gt_list "${gt_pair}")
    list(GET gt_list 0 gt_name)
    list(GET gt_list 1 gt_src)
    add_executable(${gt_name} ${gt_src})
    target_include_directories(${gt_name} PRIVATE "${WICKRA_INCLUDE_DIR}")
    target_link_libraries(${gt_name} PRIVATE "${WICKRA_LINK_LIB}")
    if(UNIX AND NOT APPLE)
        target_link_libraries(${gt_name} PRIVATE m)
    endif()
    if(WIN32)
        add_custom_command(TARGET ${gt_name} POST_BUILD
            COMMAND ${CMAKE_COMMAND} -E copy_if_different
                    "${WICKRA_RUNTIME}" "$<TARGET_FILE_DIR:${gt_name}>")
    endif()
    add_test(NAME ${gt_name} COMMAND ${gt_name} "${WICKRA_GOLDEN_DIR}")
    if(NOT WIN32)
        set_tests_properties(${gt_name} PROPERTIES
            ENVIRONMENT "LD_LIBRARY_PATH=${WICKRA_LIB_DIR};DYLD_LIBRARY_PATH=${WICKRA_LIB_DIR}")
    endif()
endforeach()

# Network examples — built (so they stay compilable) but not run in CI.
add_wickra_example(fetch_btcusdt fetch_btcusdt.c FALSE) # downloads CSVs via curl
add_wickra_example(live_binance live_binance.c FALSE)   # polls Binance REST via curl
