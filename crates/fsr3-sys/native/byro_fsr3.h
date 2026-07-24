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

typedef struct ByroFsr3Image {
    uint64_t vk_image;
    uint32_t vk_format;
    uint32_t vk_usage;
    uint32_t width;
    uint32_t height;
} ByroFsr3Image;

typedef struct ByroFsr3DispatchDesc {
    uintptr_t vk_command_buffer;
    ByroFsr3Image color;
    ByroFsr3Image depth;
    ByroFsr3Image motion_vectors;
    ByroFsr3Image exposure;
    ByroFsr3Image reactive;
    ByroFsr3Image transparency_and_composition;
    ByroFsr3Image output;
    float jitter_x;
    float jitter_y;
    float motion_vector_scale_x;
    float motion_vector_scale_y;
    uint32_t render_width;
    uint32_t render_height;
    uint32_t upscale_width;
    uint32_t upscale_height;
    float frame_time_delta_ms;
    float pre_exposure;
    bool reset;
    float camera_near;
    float camera_far;
    float camera_fov_angle_vertical;
    float view_space_to_meters_factor;
    bool enable_sharpening;
    float sharpness;
} ByroFsr3DispatchDesc;

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
uint32_t byro_fsr3_context_dispatch(
    ByroFsr3Context* context,
    const ByroFsr3DispatchDesc* desc);
uint32_t byro_fsr3_context_destroy(ByroFsr3Context** context);
const char* byro_fsr3_error_string(uint32_t error_code);

#ifdef __cplusplus
}
#endif
