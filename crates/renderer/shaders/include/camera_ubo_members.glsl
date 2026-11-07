// The members of the `CameraUBO` uniform block, in one place (#6682). Include
// this inside the braces of a block that names its own set and binding: the main
// pass binds the camera at set 1 / binding 1 and the compute passes bind it
// elsewhere, which is why a struct-only header does not fit.
//
// MUST match `GpuCamera` in `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs`
// field for field (`camera_ubo_members_match_gpu_camera`). Members only: no block
// qualifiers and no braces (`camera_ubo_members_are_declared_once`).
    mat4 viewProj;
    mat4 prevViewProj;   // Previous frame's viewProj for motion vectors (#256).
    mat4 invViewProj;    // Precomputed inverse(viewProj) for world reconstruction.
    vec4 cameraPos;      // xyz = world position, w = frame counter
    vec4 sceneFlags;     // x = RT enabled (1.0), yzw = ambient color (RGB)
    vec4 screen;         // x = width, y = height, z = fog near, w = fog far
    vec4 fog;            // xyz = linear-light fog radiance RGB, w = fog enabled (1.0)
    vec4 jitter;         // xy = sub-pixel TAA jitter in NDC, z = bitcast<f32>(render_debug_flags), w = is_exterior flag (1.0 = exterior cell, 0.0 = interior). #1125.
    // #925 / REN-D15-NEW-03 — mirror of composite's `sky_zenith.xyz`
    // (linear RGB). Used by triangle.frag's window-portal escape so
    // interior windows transmit a sky tint that matches whatever
    // `compute_sky` paints behind the world (TOD / weather cross-fade
    // already wired upstream). Pre-fix the portal site hardcoded
    // `vec3(0.6, 0.75, 1.0)` and every window looked clear-noon.
    vec4 skyTint;        // xyz = TOD/weather zenith colour, w = sun_angular_radius (rad; SkyParams::sun_angular_radius, #1023)
    vec4 sunDirection;
    vec4 dofParams;      // x = aperture half-radius (0.0 = pinhole), y = focus_dist, z = atten knee frac, w = history mode (0 moving, 1 parked, 2 parked + scene-static, 3 parked + rig-geometry-static under flicker; restir_history_mode).
    vec4 renderOrigin;   // #markarth-precision / #1496 — xyz = camera-relative render origin (cell-grid snapped); add it to a render-origin-relative position for the absolute world position. w = FSR one-frame-reset flag (NOT padding — #2164/L-10).
    uvec4 renderDebug;   // x = structured RENDER_DEBUG_* mode; y = optional bitcast RT LOD scale, z = diagnostic bits (0 = RT-LOD telemetry enable, 1 = bounded-path luminance clamp off, 2 = ReSTIR reservoir W clamp off), w = packed weather surface (low 16 wetness, high 16 snow). Legacy feature-ablation flags remain in jitter.z.
    // #3323 — the EXTERIOR TOD/weather zenith colour, carried even on
    // interior cells. `skyTint` above is `SkyParams::default()` on any
    // interior by design (#1199 / #2226: an interior must never read a
    // stale exterior sky), which is right for every consumer except
    // triangle.frag's window-portal escape, where the ray genuinely left
    // the cell. `.xyz` is read ONLY there; widening it anywhere else
    // re-opens #2226. `.w` is the sky-cubemap ready flag every `skyCube`
    // read checks — a live lane, not padding (#4299).
    vec4 exteriorSkyTint; // xyz = live exterior zenith colour, w = sky-cubemap
                          // ready flag (1.0 once `sky_cube.comp` has baked into
                          // set 1 / binding 20 this frame; 0.0 if the bake is
                          // absent, e.g. it failed to initialise under VRAM
                          // pressure — consumers must fall back, see raytrace.glsl)
