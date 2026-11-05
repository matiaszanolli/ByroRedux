// The one GLSL declaration of the Rust `GpuWaterParams` (`vulkan/water.rs`), read
// by both water stages. Every array element uses the same 368-byte std430 stride
// (23 vec4 slots). Each stage keeps its own `WaterParamsBlock`.
#ifndef BYRO_WATER_PARAMS_GLSL
#define BYRO_WATER_PARAMS_GLSL

struct WaterParams {
    // x = time (engine uptime in seconds — `TotalTime`, accumulated
    //     since engine start and never reset on cell load; f32, so wave
    //     animation quality degrades after many hours of uptime),
    //     y = WaterKind enum cast to float, z = foam_strength (0..1),
    //     w = ior (1.33 ~ 1.5)
    vec4 timing;
    // xyz = flow direction (unit), w = flow speed (world units / s)
    vec4 flow;
    // rgb = shallow_color (linear), a = fog_near
    vec4 shallow;
    // rgb = deep_color (linear), a = fog_far
    vec4 deep;
    // xy = scroll_a (UV/s — the pattern velocity; features travel along
    // +scroll, #4728), zw = scroll_b
    vec4 scroll;
    // xy = scroll_c for the authored third normal layer; zw = underwater
    // fog near/far for the camera-below-surface presentation.
    vec4 scroll_c;
    // x = uv_scale_a, y = uv_scale_b, z = shoreline_width,
    // w = wave_amplitude (WATR DATA wave_amplitude — #2240)
    vec4 tune;
    // x = fresnel_f0, y = wave_frequency (WATR DATA wave_frequency, Hz — #2240),
    // z = normal_map_index (uintBitsToFloat — sample with floatBitsToUint),
    // w = WATR Sun Specular Power (Blinn-Phong exponent)
    vec4 misc;
    // rgb = reflection_tint (WATR DATA reflection_color — tints geometry-hit
    // colour in traceWaterRay; #1069 / F-WAT-09). a = reflectivity (0..1,
    // moved from tune.w).
    vec4 tint_reflect;
    // Bindless indices for authored NAM2/NAM3/NAM4 noise layers.
    uvec4 noise_indices;
    // x = authored NAM4 UV scale; yzw = authored NAM2/3/4 amplitude scales.
    vec4 detail;
    // x = authored Skyrim noise-falloff distance; y = Blend Normals gate;
    // z = Starfield surface roughness; w = Skyrim Specular Radius.
    vec4 noise_falloff;
    // xyz = shallow/deep/surface-effect normal falloff multipliers;
    // w = packed legacy rain velocity/falloff/dampener controls.
    vec4 normal_falloff;
    // x = displacement starting size, y = radial falloff, z = dampener.
    vec4 displacement;
    // x/y/z/w = reflection/refraction/normal/specular depth weights.
    vec4 depth;
    // x/y/z/w = refraction magnitude, local specular power, reflection
    // magnitude, and sun-specular magnitude (including the authored
    // Skyrim specular-properties magnitude multiplier).
    vec4 effects;
    // xyz = Starfield per-channel extinction coefficients; zero means
    // pre-Starfield/legacy water and selects the scalar fog response.
    // w = precipitation * authored rain-response (0..4). Not a free slot —
    // same trap as VolumetricsParams.render_origin.w / GpuCamera.render_origin.w.
    vec4 absorption;
    // Starfield phytoplankton, sediment, yellow matter, oceanness.
    vec4 concentration;
    // xy = transient ripple center in world XZ, z = intensity, w = radius.
    vec4 ripple;
    // rgb = authored underwater tint, a = underwater fog amount. The
    // underwater near/far ramp lives in `scroll_c.zw` to reuse its reserved
    // legacy slots without changing the wave upload shape.
    vec4 underwater;
    // x/y = shallow/deep alpha, z/w = shallow/deep distance thresholds.
    // All zero is the legacy constant-opacity sentinel.
    vec4 alpha;
    // xy = authored mesh-water UV offset; z = flow-map index bit-cast;
    // w = authored flow-map scale.
    vec4 uv_offset;
    // x = FO4+/Creation-2 WATR Depth Amount; y = WaterNormalEncoding
    // (0 tangent normal, 1 offset noise); zw reserved. x is not a
    // substitute for the fog distances in shallow.a/deep.a.
    vec4 optical;
};

#endif // BYRO_WATER_PARAMS_GLSL
