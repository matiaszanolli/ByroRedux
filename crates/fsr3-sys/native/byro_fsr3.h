#pragma once

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ByroFsr3Context ByroFsr3Context;

typedef struct ByroFsr3Version {
    uint32_t major;
    uint32_t minor;
    uint32_t patch;
    uint64_t provider_id;
} ByroFsr3Version;

typedef struct ByroFsr3CreateDesc {
    uintptr_t vk_device;
    uintptr_t vk_physical_device;
    uintptr_t vk_get_device_proc_addr;
    uint32_t max_render_width;
    uint32_t max_render_height;
    uint32_t max_upscale_width;
    uint32_t max_upscale_height;
    bool high_dynamic_range;
    bool debug_checking;
} ByroFsr3CreateDesc;

uint32_t byro_fsr3_query_version(ByroFsr3Version* out_version);
uint32_t byro_fsr3_query_render_resolution(
    uint32_t display_width,
    uint32_t display_height,
    uint32_t quality_mode,
    uint32_t* out_render_width,
    uint32_t* out_render_height);
uint32_t byro_fsr3_query_jitter_phase_count(
    uint32_t render_width,
    uint32_t display_width,
    int32_t* out_phase_count);
uint32_t byro_fsr3_query_jitter_offset(
    uint32_t index,
    int32_t phase_count,
    float* out_x,
    float* out_y);
uint32_t byro_fsr3_context_create(
    ByroFsr3Context** out_context,
    const ByroFsr3CreateDesc* desc);
uint32_t byro_fsr3_context_destroy(ByroFsr3Context** context);
const char* byro_fsr3_error_string(uint32_t error_code);

#ifdef __cplusplus
}
#endif
