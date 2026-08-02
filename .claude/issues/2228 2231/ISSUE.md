# Issues 2228 + 2231

## #2228: REN-D3-01 -- GpuFogVolume has a size/align pin but no field-order Rust<->GLSL lockstep test
- Severity: MEDIUM
- Labels: bug, renderer, medium, vulkan, tech-debt
- State: OPEN
- Dimension: 3 (GPU structs)
- Location: crates/renderer/src/vulkan/volumetrics.rs (GpuFogVolume, line 127; size/align asserts ~2009-2010); crates/renderer/shaders/volumetrics_inject.comp (struct GpuFogVolume, line 124)
- Fields currently agree (center_shape, half_extents_extinction, inverse_rotation, albedo_edge) but nothing catches a future reorder -- same gap class as GpuInstance/GpuMaterial history (feedback_shader_struct_sync.md).
- Suggested fix: add a lockstep test (reuse the GpuInstance/GpuMaterial mechanism -- offset_of! reflection) asserting each GpuFogVolume field's byte offset matches its GLSL struct position.

## #2231: REN-D5-03 -- Boot-generated volumetric density noise is regenerated on the CPU on every window resize
- Severity: MEDIUM
- Labels: bug, renderer, medium, performance
- State: OPEN
- Dimension: 5 (Memory)
- Location: crates/renderer/src/vulkan/volumetrics.rs (VolumetricsPipeline::initialize_layouts, line ~1190; calls generate_density_noise at 1197-1198); crates/renderer/src/vulkan/context/resize.rs:808 (new_volumetrics.initialize_layouts(...) on swapchain-recreate path)
- initialize_layouts's own doc comment says "Call once after new()" but resize.rs constructs a new VolumetricsPipeline and calls initialize_layouts on every window resize -- regenerates both base and detail procedural density-noise volumes from scratch on CPU (~10^7 hash evals), a real bounded stall on every resize.
- Noise is resolution-independent (BASE_NOISE_SIZE / DETAIL_NOISE_SIZE, not tied to swapchain extent) -- regenerating it on resize is pure waste.
- Suggested fix: cache the generated noise texels (or the noise itself) across resize and only re-upload/rebind rather than regenerating.
