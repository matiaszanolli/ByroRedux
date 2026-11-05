// The one GLSL declaration of the Rust `GpuLight`
// (`vulkan/scene_buffer/gpu_types.rs`). A shader that reads lights includes this
// and keeps only its own `LightBuffer` block, numbered for its own descriptor
// set. Struct-only, so any stage can include it.
#ifndef BYRO_GPU_LIGHT_GLSL
#define BYRO_GPU_LIGHT_GLSL

struct GpuLight {
    vec4 position_radius;  // xyz = position, w = radius
    vec4 color_type;       // rgb=color, w=type (0=point, 1=spot, 2=directional)
    vec4 direction_angle;  // xyz = direction, w = spot angle cosine
    vec4 params;           // x = falloff, y = source radius, z = visibility bits, w = attenuation model
};

#endif // BYRO_GPU_LIGHT_GLSL
