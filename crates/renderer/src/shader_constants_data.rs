// Single source of truth for constants that live in both Rust and GLSL.
// This file is included (via include!) by both:
//   - crates/renderer/src/shader_constants.rs   (library)
//   - crates/renderer/build.rs                   (build script → generates shader_constants.glsl)
//
// When updating a value here, rebuild with `cargo build -p byroredux-renderer`
// to regenerate the GLSL header, then recompile the affected GLSL shaders.

// Cluster grid
pub const CLUSTER_TILES_X: u32 = 16;
pub const CLUSTER_TILES_Y: u32 = 9;
pub const CLUSTER_SLICES_Z: u32 = 24;
pub const CLUSTER_NEAR: f32 = 0.1;
pub const CLUSTER_FAR_FLOOR: f32 = 10_000.0;
pub const CLUSTER_FAR_FALLBACK: f32 = 50_000.0;
// Raised from 32 → 128 after the LIGH `falloff_exponent` plumb-through
// (which extended the per-light visible range to `radius * 2.5`)
// exposed densely-lit FO4 interior cells overflowing the cap. The
// cluster cull's `atomicAdd` gives arbitrary slot ordering when the
// actual light count exceeds the cap, so adjacent clusters drop
// DIFFERENT subsets of lights — producing visible tile boundaries on
// floors / walls (Institute Bioscience cargo room was the canonical
// regression). Buffer grows from 3456 * 32 * 4 = 442 KB to 3456 * 128
// * 4 = 1.7 MB — trivial against the multi-GB VRAM budget.
//
// 128 proved insufficient once the R2 GPU telemetry was exercised on
// Starfield: Cydonia measured 305 overlaps in its densest cluster and dropped
// 3,729 light references across 52 clusters. 512 retains the complete measured
// set with 68% headroom. The flat index list grows to 3456 * 512 * 4 = 6.75 MB
// per frame-in-flight and the workgroup-local index array to 2 KB, both modest
// on the renderer's 6 GB minimum target.
pub const MAX_LIGHTS_PER_CLUSTER: u32 = 512;

// ReSTIR reservoir word packing. The low ten bits store a scene-light index;
// their all-ones value is reserved as the packed invalid sentinel, so the
// upload capacity is a COUNT of 1023 and valid uploaded indices are 0..=1022.
// The remaining bits hold the stable surface ID used by spatial reuse.
//
// Keep this complete contract here rather than duplicating masks and shift
// literals between scene-buffer sizing and triangle.frag (#2778).
pub const RESERVOIR_LIGHT_BITS: u32 = 10;
pub const RESERVOIR_LIGHT_MASK: u32 = (1u32 << RESERVOIR_LIGHT_BITS) - 1;
pub const RESERVOIR_SURFACE_MASK: u32 = u32::MAX >> RESERVOIR_LIGHT_BITS;
pub const MAX_LIGHTS: usize = RESERVOIR_LIGHT_MASK as usize;

const _: () = {
    assert!(MAX_LIGHTS == 1023);
    assert!(RESERVOIR_SURFACE_MASK == 0x003f_ffff);
};

// Ray-query alpha-skip walk budget. Bounds any loop that walks a ray query
// past alpha-tested/uncovered geometry (reflection self-skips, water's
// foliage-cutout skips, opaque shadow-transmittance layering) — eight
// layers is enough for realistic authored foliage/grate stacks without
// letting a pathological stack turn one ray into an unbounded walk.
//
// #2265 / TD7-001 — previously hand-redeclared as `MAX_TRANSPARENT_SKIPS`
// (raytrace.glsl, water.frag) and `MAX_OPAQUE_LAYERS` (shadow_transport.glsl)
// at each call site; consolidated here so a future tuning pass has one
// value to change instead of three to find.
pub const MAX_ALPHA_SKIP_LAYERS: u32 = 8;

// Vertex layout (global SSBO)
pub const VERTEX_STRIDE_FLOATS: u32 = 26;
// Skinned-vertex OUTPUT stride (`SkinSlot::output_buffer`) — position
// only, deliberately NOT the full 26-float input layout.
//
// #2170 — the slot output used to mirror the input stride so a deferred
// "Phase 3" (raster reading pre-skinned vertices as a VBO) could swap in
// without a layout change. Phase 3 never landed, `create_slot` omits the
// `VERTEX_BUFFER` usage it would need anyway (#681/MEM-2-6), and the only
// live consumer is the skinned-BLAS build, which reads the buffer as
// `R32G32B32_SFLOAT` and touches 12 of every 104 bytes. Provisioning the
// other 92 cost an 8.7x over-allocation per slot plus the bandwidth to
// write pass-through lanes nothing reads.
//
// If Phase 3 is ever revived it needs the full layout back AND the
// `VERTEX_BUFFER` usage bit — that is one commit, not a silent
// dependency on this constant.
//
// 3 floats = 12 B satisfies the AS-build requirement that `vertexStride`
// be a multiple of the vertex format's component size (4 B for f32).
pub const SKIN_OUTPUT_STRIDE_FLOATS: u32 = 3;
// position(0..3) color RGBA(3..7) normal(7..10) uv(10..12) — see
// crates/renderer/src/vertex.rs.
pub const VERTEX_COLOR_OFFSET_FLOATS: u32 = 3;
pub const VERTEX_NORMAL_OFFSET_FLOATS: u32 = 7;
pub const VERTEX_UV_OFFSET_FLOATS: u32 = 10;
pub const VERTEX_TANGENT_OFFSET_FLOATS: u32 = 22;

// Skinning — see `byroredux_core::ecs::components::skinned_mesh::MAX_BONES_PER_MESH`
// for the vanilla-content survey that fixes this ceiling at 144 (FO76 prewardress = 133).
pub const MAX_BONES_PER_MESH: u32 = 144;

// Skin-compute workgroup width. Both `skin_vertices.comp` (1 invocation per
// vertex) and `skin_palette.comp` (1 invocation per bone slot) run a 1D
// 64-wide dispatch; the Rust dispatch group-count math in
// `vulkan/skin_compute.rs` (`*.div_ceil(SKIN_WORKGROUP_SIZE)`) re-exports
// this same const so the layout qualifier and the group count can't drift.
// Distinct from `WORKGROUP_X = 8` (the 2D image-pass tile width) — skinning
// is a flat 1D dispatch. Emitted with no `u` suffix so it works in the
// `layout(local_size_x = SKIN_WORKGROUP_SIZE)` qualifier. #1758 / TD7-001.
pub const SKIN_WORKGROUP_SIZE: u32 = 64;

// Material kind enum (GpuMaterial.materialKind).
// Authoritative Rust-side values live in `scene_buffer/constants.rs`. #1401.
pub const MATERIAL_KIND_GLASS: u32 = 100;
pub const MATERIAL_KIND_EFFECT_SHADER: u32 = 101;
pub const MATERIAL_KIND_NO_LIGHTING: u32 = 102;
pub const MATERIAL_KIND_FIRE_REFRACTION: u32 = 103;

// Explicit geometry visibility layers shared with the ECS emitter contract.
// Each TLAS instance carries exactly one category bit. `GpuLight.params.z`
// stores any union of these bits as an exactly representable f32 integer.
pub const VISIBILITY_LAYER_ARCHITECTURE: u32 =
    byroredux_core::lighting::VisibilityMask::ARCHITECTURE.bits() as u32;
pub const VISIBILITY_LAYER_STATIC_PROP: u32 =
    byroredux_core::lighting::VisibilityMask::STATIC_PROP.bits() as u32;
pub const VISIBILITY_LAYER_DYNAMIC_ACTOR: u32 =
    byroredux_core::lighting::VisibilityMask::DYNAMIC_ACTOR.bits() as u32;
pub const VISIBILITY_LAYER_FOLIAGE: u32 =
    byroredux_core::lighting::VisibilityMask::FOLIAGE.bits() as u32;
pub const VISIBILITY_LAYER_GLASS: u32 =
    byroredux_core::lighting::VisibilityMask::GLASS.bits() as u32;
pub const VISIBILITY_LAYER_EFFECT: u32 =
    byroredux_core::lighting::VisibilityMask::EFFECT.bits() as u32;
pub const VISIBILITY_MASK_ALL_OPAQUE: u32 =
    byroredux_core::lighting::VisibilityMask::ALL_OPAQUE.bits() as u32;
pub const VISIBILITY_MASK_SOLID: u32 =
    byroredux_core::lighting::VisibilityMask::SOLID.bits() as u32;
pub const VISIBILITY_MASK_FULL: u32 = byroredux_core::lighting::VisibilityMask::FULL.bits() as u32;

pub const ATTENUATION_MODEL_LEGACY_SOFT_RANGE: u32 =
    byroredux_core::lighting::AttenuationModel::LegacySoftRange as u32;
pub const ATTENUATION_MODEL_INVERSE_SQUARE: u32 =
    byroredux_core::lighting::AttenuationModel::InverseSquare as u32;
pub const WORLD_UNITS_PER_METER: f32 = byroredux_core::lighting::BETHESDA_UNITS_PER_METER;
pub const ADIABATIC_FLAME_TEMPERATURE_K: f32 =
    byroredux_core::combustion::ADIABATIC_FLAME_TEMPERATURE_K;
pub const COMBUSTION_REACTION_RATE_PER_SECOND: f32 =
    byroredux_core::combustion::REACTION_RATE_PER_SECOND;
pub const COMBUSTION_RICH_SOOT_YIELD: f32 = byroredux_core::combustion::RICH_SOOT_YIELD;
pub const COMBUSTION_LEAN_SOOT_YIELD: f32 = byroredux_core::combustion::LEAN_SOOT_YIELD;
pub const COMBUSTION_SOOT_OXIDATION_RATE_PER_SECOND: f32 =
    byroredux_core::combustion::SOOT_OXIDATION_RATE_PER_SECOND;
pub const COMBUSTION_SOOT_SINGLE_SCATTER_ALBEDO: f32 =
    byroredux_core::combustion::SOOT_SINGLE_SCATTER_ALBEDO;
pub const COMBUSTION_SOOT_OXIDATION_START_TEMPERATURE_K: f32 =
    byroredux_core::combustion::SOOT_OXIDATION_START_TEMPERATURE_K;
pub const COMBUSTION_SOOT_OXIDATION_FULL_TEMPERATURE_K: f32 =
    byroredux_core::combustion::SOOT_OXIDATION_FULL_TEMPERATURE_K;
pub const EXPLOSION_EXPANSION_TIME_SECONDS: f32 =
    byroredux_core::combustion::EXPLOSION_EXPANSION_TIME_SECONDS;
pub const EXPLOSION_IMPULSE_DURATION_SECONDS: f32 =
    byroredux_core::combustion::EXPLOSION_IMPULSE_DURATION_SECONDS;
pub const COMBUSTION_OVERPRESSURE_DISSIPATION_PER_SECOND: f32 =
    byroredux_core::combustion::OVERPRESSURE_DISSIPATION_PER_SECOND;
pub const COMBUSTION_MAX_PRESSURE_ACCELERATION_MPS2: f32 =
    byroredux_core::combustion::MAX_PRESSURE_ACCELERATION_MPS2;
pub const COMBUSTION_MAX_DILUTION_RATE_PER_SECOND: f32 =
    byroredux_core::combustion::MAX_DILUTION_RATE_PER_SECOND;
pub const COMBUSTION_VORTICITY_CONFINEMENT_SPEED_MPS: f32 =
    byroredux_core::combustion::VORTICITY_CONFINEMENT_SPEED_MPS;
pub const COMBUSTION_MAX_VORTICITY_ACCELERATION_MPS2: f32 =
    byroredux_core::combustion::MAX_VORTICITY_ACCELERATION_MPS2;
pub const COMBUSTION_TURBULENCE_COARSE_EDDY_SCALE_METERS: f32 =
    byroredux_core::combustion::TURBULENCE_COARSE_EDDY_SCALE_METERS;
pub const COMBUSTION_TURBULENCE_DETAIL_EDDY_SCALE_METERS: f32 =
    byroredux_core::combustion::TURBULENCE_DETAIL_EDDY_SCALE_METERS;
pub const COMBUSTION_TURBULENCE_COARSE_RISE_SPEED_MPS: f32 =
    byroredux_core::combustion::TURBULENCE_COARSE_RISE_SPEED_MPS;
pub const COMBUSTION_TURBULENCE_DETAIL_RISE_SPEED_MPS: f32 =
    byroredux_core::combustion::TURBULENCE_DETAIL_RISE_SPEED_MPS;
pub const COMBUSTION_AEROSOL_DISSIPATION_PER_SECOND: f32 =
    byroredux_core::combustion::AEROSOL_DISSIPATION_PER_SECOND;
pub const COMBUSTION_AEROSOL_LINGER_SECONDS: f32 =
    byroredux_core::combustion::AEROSOL_LINGER_SECONDS;
pub const COMBUSTION_AEROSOL_LIFT_ACCELERATION_MPS2: f32 =
    byroredux_core::combustion::AEROSOL_LIFT_ACCELERATION_MPS2;
pub const COMBUSTION_AEROSOL_LIFT_EXTINCTION_SCALE: f32 =
    byroredux_core::combustion::AEROSOL_LIFT_EXTINCTION_SCALE;
pub const COMBUSTION_COOLED_AEROSOL_SOURCE_TEMPERATURE_K: f32 =
    byroredux_core::combustion::COOLED_AEROSOL_SOURCE_TEMPERATURE_K;
pub const COMBUSTION_EXPLOSION_SMOKE_EXTINCTION_SCALE: f32 =
    byroredux_core::combustion::EXPLOSION_SMOKE_EXTINCTION_SCALE;
pub const COMBUSTION_FUEL_VAPOUR_REMOVAL_PER_SECOND: f32 =
    byroredux_core::combustion::FUEL_VAPOUR_REMOVAL_PER_SECOND;
pub const COMBUSTION_RADIANCE_REMOVAL_PER_SECOND: f32 =
    byroredux_core::combustion::RADIANCE_REMOVAL_PER_SECOND;
pub const COMBUSTION_THERMAL_COOLING_PER_SECOND: f32 =
    byroredux_core::combustion::THERMAL_COOLING_PER_SECOND;
pub const COMBUSTION_THERMAL_BUOYANCY_ACCELERATION_MPS2: f32 =
    byroredux_core::combustion::THERMAL_BUOYANCY_ACCELERATION_MPS2;
pub const COMBUSTION_VELOCITY_DAMPING_PER_SECOND: f32 =
    byroredux_core::combustion::VELOCITY_DAMPING_PER_SECOND;
pub const COMBUSTION_REACTION_HEAT_RESPONSE: f32 =
    byroredux_core::combustion::REACTION_HEAT_RESPONSE;
pub const FLAME_FUEL_BOUNDARY_HEIGHT_FRACTION: f32 =
    byroredux_core::combustion::FLAME_FUEL_BOUNDARY_HEIGHT_FRACTION;
pub const FLAME_REACTION_ZONE_HEIGHT_FRACTION: f32 =
    byroredux_core::combustion::FLAME_REACTION_ZONE_HEIGHT_FRACTION;
pub const FLAME_REACTION_ZONE_FADE_START_FRACTION: f32 =
    byroredux_core::combustion::FLAME_REACTION_ZONE_FADE_START_FRACTION;
pub const FLAME_SOURCE_LATERAL_SPEED_MPS: f32 =
    byroredux_core::combustion::FLAME_SOURCE_LATERAL_SPEED_MPS;
pub const FLAME_SOURCE_VELOCITY_RESPONSE_PER_SECOND: f32 =
    byroredux_core::combustion::FLAME_SOURCE_VELOCITY_RESPONSE_PER_SECOND;

// Transported-combustion -> surface-light reduction. The froxel injector
// accumulates fixed-point radiant moments into this camera-centred grid; Rust
// drains the exact same ABI after the frame-slot fence. Keep the dimensions,
// domain, and quantization here so neither side can silently reinterpret a
// bin. 8 x 4 x 8 resolves distinct room-scale fires while keeping readback to
// 8 KiB per frame-in-flight slot (256 bins x 32 bytes).
pub const COMBUSTION_LIGHT_GRID_X: u32 = 8;
pub const COMBUSTION_LIGHT_GRID_Y: u32 = 4;
pub const COMBUSTION_LIGHT_GRID_Z: u32 = 8;
pub const COMBUSTION_LIGHT_GRID_COUNT: u32 =
    COMBUSTION_LIGHT_GRID_X * COMBUSTION_LIGHT_GRID_Y * COMBUSTION_LIGHT_GRID_Z;
pub const COMBUSTION_LIGHT_HALF_EXTENT_XZ_METERS: f32 = 32.0;
pub const COMBUSTION_LIGHT_HALF_EXTENT_Y_METERS: f32 = 16.0;
// 1/4096 radiant-intensity resolution keeps the 1e-3 irradiance cutoff
// representable even for centimetre-scale test scenes. Per-invocation values
// are still capped to 16 intensity units by the shader's 65535-word guard;
// bins accumulate those bounded contributions in u32.
pub const COMBUSTION_LIGHT_FIXED_SCALE: f32 = 4096.0;
// Luminous volume needs finer resolution: 1/65536 m^3 is a ~2.4 cm sphere.
pub const COMBUSTION_LIGHT_VOLUME_FIXED_SCALE: f32 = 65536.0;

const _: () = {
    assert!(COMBUSTION_LIGHT_GRID_COUNT == 256);
    assert!(COMBUSTION_LIGHT_FIXED_SCALE > 0.0);
    assert!(COMBUSTION_LIGHT_VOLUME_FIXED_SCALE > 0.0);
};

// Shared direct-shadow distance contract. These values apply to every
// environment; cell kind may change light sources and GI inputs, never the
// direct-light shadow reach. The trace distance covers the complete fade
// interval so the last shadowed samples taper out instead of ending early.
pub const SHADOW_FADE_START: f32 = 8_000.0;
pub const SHADOW_FADE_END: f32 = 12_000.0;
pub const DIRECTIONAL_SHADOW_TRACE_DISTANCE: f32 = SHADOW_FADE_END;

const _: () = {
    assert!(SHADOW_FADE_START >= 0.0);
    assert!(SHADOW_FADE_START < SHADOW_FADE_END);
    assert!(DIRECTIONAL_SHADOW_TRACE_DISTANCE >= SHADOW_FADE_END);
};

// Vulkan's instance mask is eight bits. Pin the complete cross-CPU/GPU
// contract and make accidental layer collisions a compile-time failure.
const _: () = {
    assert!(VISIBILITY_MASK_FULL <= 0xFF);
    assert!(VISIBILITY_MASK_FULL.count_ones() == 6);
    assert!(VISIBILITY_MASK_ALL_OPAQUE & VISIBILITY_LAYER_GLASS == 0);
    assert!(VISIBILITY_MASK_SOLID == VISIBILITY_MASK_ALL_OPAQUE | VISIBILITY_LAYER_GLASS);
    assert!(VISIBILITY_MASK_SOLID & VISIBILITY_LAYER_EFFECT == 0);
    assert!(
        VISIBILITY_MASK_FULL
            == VISIBILITY_MASK_ALL_OPAQUE | VISIBILITY_LAYER_GLASS | VISIBILITY_LAYER_EFFECT
    );
};

// Glass / IOR ray-work telemetry. GLASS_RAY_COST is the tier-0 estimate
// (one optional reflection + three refraction queries); the shader records
// two more estimated queries per adaptive quality tier as its interface
// allowance grows from 2 to 8. GLASS_RAY_BUDGET is retained as the tier-3
// controller comparison ceiling. The atomic counter is deliberately not a
// per-fragment admission pool: unordered winners split alpha glass between
// IOR and Fresnel paths and create permanent stipple. The controller chooses
// one coherent quality tier for the next frame instead.
pub const GLASS_RAY_BUDGET: u32 = 2_097_152;
pub const GLASS_RAY_COST: u32 = 4;

// First-bounce GI candidate pool. The shader ranks these locally, then stops
// after the first two VISIBLE contributors. Keeping eight candidates avoids a
// black bounce when the strongest one or two lamps are behind a wall without
// paying eight shadow rays on the common path.
pub const GI_HIT_LIGHT_CAP: u32 = 8;

// Chroma-preserving luminance ceiling for the complete one-pixel bounded-path
// sample before it enters SVGF. A single secondary GGX hit can line up with a
// point light's delta direction and produce a mathematically valid but extreme
// value at 1 spp; the temporal/spatial filters cannot average that heavy tail
// before it becomes a large low-frequency stain. Keep ordinary bounce energy
// untouched and bound only the outlier tail at the denoiser boundary.
pub const GI_SAMPLE_LUMINANCE_CLAMP: f32 = 1.0;

// Caustic accumulation
pub const CAUSTIC_FIXED_SCALE: f32 = 65536.0;

// Compute workgroup sizes (bloom, volumetrics, SSAO, TAA)
pub const WORKGROUP_X: u32 = 8;
pub const WORKGROUP_Y: u32 = 8;
pub const WORKGROUP_Z: u32 = 8;

// Clustered light culling thread count (one warp/wavefront wide on
// every IHV: NVIDIA = 32, AMD = 64 wavefront but a 32-thread workgroup
// still maps cleanly to half a wave, Intel = 8/16/32 SIMD width
// negotiates fine at this size). Consumed by `cluster_cull.comp` via
// the `#include`d `#define` for both `layout(local_size_x = ...)` and
// the thread-strided light scan loop. Omitted `u` suffix on the
// generated `#define` so it can be used in the layout qualifier
// (GLSL allows int literals but not `uint` literals there).
pub const THREADS_PER_CLUSTER: u32 = 32;

// M58 — bloom contribution coefficient. 0.15 is a hand-tuned perceptual
// constant (tuned down from 0.20 on Prospector saloon: sun-lit windows +
// chandelier globes were producing halos that bled too far across
// walls), chosen so Bethesda's LDR-authored emissives (0–1 monitor-space
// range, not HDR cd/m²) read as obviously bloomed without flooding dim
// surfaces.
//
// It is NOT tuned to cancel the upsample pyramid's own DC gain (see
// `bloom_upsample.comp`'s #1275 note) — that additive, non-renormalised
// up-chain carries an inherent ~5× DC gain at mip 0 for a
// spatially-uniform bright source. The two effects compose rather than
// cancel: effective contribution to `composite.frag`'s `combined` = 5×
// (pyramid) × 0.15 (this constant) = 0.75× the local blurred average per
// pixel — about 19× Frostbite SIGGRAPH 2015's own 0.04 reference, which
// assumes a renormalised (unit-gain) pyramid this one isn't. Absorbing
// the 5× gain down to Frostbite's reference would take ≈0.008, not 0.15
// — this constant is doing LDR-authoring compensation, not gain
// cancellation. `bloom_downsample.comp`'s `DownsampleParams` carries no
// bright-pass threshold or Karis average, so this is a broadband lift on
// the local average, not a highlight-only glow.
//
// Consumed by `composite.frag` via the `#include`d `#define`; mirrored
// here so Rust-side `bloom::DEFAULT_BLOOM_INTENSITY` stays in lockstep.
// See `feedback_color_space.md` for why we don't HDR-boost emissives
// globally instead.
pub const BLOOM_INTENSITY: f32 = 0.15;

// M55 — default volumetric far plane in Bethesda world units. Runtime
// shaders now receive this through their UBO because the reach is configurable;
// the generated define remains as the canonical default for diagnostics and
// shader-contract tests.
//
// The renderer deliberately preserves Gamebryo coordinates (70 units per
// metre). The original 200.0 value was documented and tuned as 200 metres but
// consumed directly beside world-space positions, truncating the volume at
// 2.86 m. 8,960 units gives the default 128 m reach; volumetric density
// is converted from 1/m to 1/world-unit on the host side in volumetrics.rs.
pub const VOLUME_FAR: f32 = 8_960.0;

// Per-instance flag bits on `GpuInstance.flags` (lower 16 bits — the
// upper 16 bits pack the terrain-tile slot per
// `INSTANCE_TERRAIN_TILE_SHIFT/MASK`). Authoritative Rust-side values
// live in `crates/renderer/src/vulkan/scene_buffer/constants.rs`; this
// shader-side mirror is pinned equal via
// `instance_flag_bits_match_scene_buffer_consts` so the two layers
// can't drift. See #1190 (TD4-NEW-01). The reserved PRESKINNED bit
// (bit 6) is not emitted as a shader-side flag because nothing in
// GLSL reads it today; if it grows a consumer, add the bit + a
// matching `#define` to keep the include the single source of truth.
pub const INSTANCE_FLAG_NON_UNIFORM_SCALE: u32 = 1 << 0;
pub const INSTANCE_FLAG_ALPHA_BLEND: u32 = 1 << 1;
pub const INSTANCE_FLAG_CAUSTIC_SOURCE: u32 = 1 << 2;
pub const INSTANCE_FLAG_TERRAIN_SPLAT: u32 = 1 << 3;
// Bit offset/mask for the `RenderLayer` classification packed into
// bits 4..5 of `GpuInstance.flags` (#2045 / TD7-101). Previously
// hand-written as `INST_RENDER_LAYER_SHIFT`/`_MASK` directly in
// `triangle.frag` with no lockstep test, unlike every other
// `INSTANCE_FLAG_*` bit; pinned equal to
// `scene_buffer::constants::INSTANCE_RENDER_LAYER_SHIFT`/`_MASK` via
// `instance_render_layer_bits_match_scene_buffer_consts`. Consumed by
// the fragment shader's `DBG_VIZ_RENDER_LAYER` debug-viz branch.
pub const INSTANCE_RENDER_LAYER_SHIFT: u32 = 4;
pub const INSTANCE_RENDER_LAYER_MASK: u32 = 0x3;
pub const INSTANCE_FLAG_FLAT_SHADING: u32 = 1 << 7;
// bit 8 — diffuse texture carries a genuine authored alpha channel
// (BC2/BC3/BC7/RGBA). Set CPU-side from the cached `handle_has_alpha`
// classification (`format_has_alpha`, which excludes BC1_RGBA). When
// CLEAR, `triangle.frag` pins `texColor.a` to 1.0 (unless an alpha test
// is active) so a BC1 3-colour-block texel (1-bit punch-through, not
// authored alpha) can't leak transparency into the discard / decalWeight
// / finalAlpha paths on a pure-blend mesh. See #1653.
pub const INSTANCE_FLAG_DIFFUSE_ALPHA: u32 = 1 << 8;

// Per-material flag bits on `GpuMaterial.materialFlags`. Authoritative
// Rust-side values live in `crates/renderer/src/vulkan/material.rs`
// (`material_flag::*`); this shader-side mirror is pinned equal via
// `material_flag_bits_match_material_consts`. See #1190. build.rs emits
// these as `#define`s into `shader_constants.glsl`, so `triangle.frag`
// MUST get them from the `#include` — never hand-write them.
//
// Bits 5-9 (the #1147 Phase 2a / #1248-#1250 Disney BSDF + SSS +
// model-space-normals suite) were previously hand-written `#define`s in
// `triangle.frag` with no lockstep test; #1285 brought them into the
// generated header alongside bits 0-4.
pub const MAT_FLAG_VERTEX_COLOR_EMISSIVE: u32 = 1 << 0;
pub const MAT_FLAG_EFFECT_SOFT: u32 = 1 << 1;
pub const MAT_FLAG_EFFECT_PALETTE_COLOR: u32 = 1 << 2;
pub const MAT_FLAG_EFFECT_PALETTE_ALPHA: u32 = 1 << 3;
pub const MAT_FLAG_EFFECT_LIT: u32 = 1 << 4;
pub const MAT_FLAG_PBR_BSDF: u32 = 1 << 5;
pub const MAT_FLAG_TRANSLUCENCY: u32 = 1 << 6;
pub const MAT_FLAG_MODEL_SPACE_NORMALS: u32 = 1 << 7;
pub const MAT_FLAG_TRANSLUCENCY_THICK_OBJECT: u32 = 1 << 8;
pub const MAT_FLAG_TRANSLUCENCY_MIX_ALBEDO: u32 = 1 << 9;
// Non-occluding glass sheet/shell. This is a canonical behavior flag, not a
// BGEM provenance bit: any source translator may select it.
pub const MAT_FLAG_THIN_GLASS: u32 = 1 << 11;
// #2826 (REN-D19-02) — set when the bound MODEL_SPACE_NORMALS map's blue
// channel carries authored Z (three-channel FO4 `_msn`, BC3) rather than
// being empty (two-channel `_msn`, BC1). Distinguished from the DDS
// compression format at texture-load time; see `material_flag::
// MSN_HAS_AUTHORED_Z` in `vulkan/material.rs` for the full rationale.
pub const MAT_FLAG_MSN_HAS_AUTHORED_Z: u32 = 1 << 12;
// `MAT_FLAG_EFFECT_LI_SHIFT` — bit offset for the 8-bit
// `BSEffectShaderProperty.lighting_influence` byte packed into bits
// 16-23 of `materialFlags`. Extract via
// `float((mat.materialFlags >> MAT_FLAG_EFFECT_LI_SHIFT) & 0xFFu) / 255.0`.
// Paired with `material_flag::EFFECT_LI_SHIFT` (Rust) and pinned by
// `material_flag_bits_match_material_consts`. See #890 Stage 2.
pub const MAT_FLAG_EFFECT_LI_SHIFT: u32 = 16;

// NOTE: `material_flag::BGSM_AUTHORED` (Rust-side bit 10) is
// NOT mirrored here — the shader is format-agnostic and doesn't
// branch on material provenance. BGSM → standardized PBR
// translation happens in `merge_external_material`, which writes
// `metalness_override` / `roughness_override` on the raw-tier
// `ImportedMesh`; `translate_material` then resolves those into
// `Material.{metalness,roughness}`. The Rust-side flag rides
// through for debug-server inspection only.
// See `feedback_format_translation.md`.

// High bit OR'd into `GpuMaterial.glossMapIndex` to tell the fragment shader
// "the gloss/smoothness mask lives in the NORMAL map's ALPHA channel"
// (Skyrim/Gamebryo normal-alpha-as-spec). Set per-draw CPU-side in
// `byroredux::render::static_meshes`; the shader masks it off for the index
// (`glossMapIndex & ~NORMAL_ALPHA_SPEC_BIT`) and samples `.a` instead of `.r`.
// Lockstep with `triangle.frag` and `byroredux::material_translate`, which
// re-exports this value rather than re-declaring it (#1500 / REN2-15).
pub const NORMAL_ALPHA_SPEC_BIT: u32 = 0x8000_0000;

// Water motion-kind enum (WATR-driven, mapped per-WATR record).
// Lockstep with `water.frag` and `byroredux/src/cell_loader/water.rs`.
pub const WATER_CALM: u32 = byroredux_core::ecs::components::water::WaterKind::Calm as u32;
pub const WATER_RIVER: u32 = byroredux_core::ecs::components::water::WaterKind::River as u32;
pub const WATER_RAPIDS: u32 = byroredux_core::ecs::components::water::WaterKind::Rapids as u32;
pub const WATER_WATERFALL: u32 =
    byroredux_core::ecs::components::water::WaterKind::Waterfall as u32;
pub const WATER_LAVA: u32 = byroredux_core::ecs::components::water::WaterKind::Lava as u32;
pub const DEFAULT_WATER_WAVE_AMPLITUDE: f32 =
    byroredux_core::ecs::components::water::DEFAULT_WATER_WAVE_AMPLITUDE;
pub const DEFAULT_WATER_WAVE_FREQUENCY: f32 =
    byroredux_core::ecs::components::water::DEFAULT_WATER_WAVE_FREQUENCY;
pub const STARFIELD_WATER_CONCENTRATION_REFERENCE: f32 =
    byroredux_core::ecs::components::water::STARFIELD_WATER_CONCENTRATION_REFERENCE;

// Local fog-volume clustering (M55/Session 62). Lockstep with
// `volumetrics_inject.comp`'s `sampleLocalMedium` and
// `vulkan::volumetrics`, which derive their own `usize` copies from these
// (#2229 / REN-D3-02 — previously hand-duplicated as GLSL literals with no
// shared source, the same defect class as #1190/#1401).
/// Camera-centered world-space cluster resolution used for local fog.
pub const FOG_VOLUME_CLUSTER_DIM: u32 = 16;
/// Bounded primitive references per cluster. Overflow keeps the nearest
/// volumes because the CPU input list is distance-sorted.
pub const MAX_FOG_VOLUMES_PER_CLUSTER: u32 = 8;

// Canonical local-medium profiles. Source-format/game interpretation ends at
// the FogVolume -> GpuFogVolume boundary; both Rust and GLSL consume these
// generated renderer-domain identifiers rather than maintaining parallel
// numeric tables.
pub const FOG_VOLUME_PROFILE_HOMOGENEOUS: f32 =
    byroredux_core::ecs::FogProfile::Homogeneous as u32 as f32;
pub const FOG_VOLUME_PROFILE_SMOKE: f32 =
    byroredux_core::ecs::FogProfile::Smoke as u32 as f32;
pub const FOG_VOLUME_PROFILE_FLAME: f32 =
    byroredux_core::ecs::FogProfile::Flame as u32 as f32;
pub const FOG_VOLUME_PROFILE_EXPLOSION: f32 =
    byroredux_core::ecs::FogProfile::Explosion as u32 as f32;
pub const FOG_VOLUME_PROFILES: &[(&str, f32)] = &[
    (
        "FOG_VOLUME_PROFILE_HOMOGENEOUS",
        FOG_VOLUME_PROFILE_HOMOGENEOUS,
    ),
    ("FOG_VOLUME_PROFILE_SMOKE", FOG_VOLUME_PROFILE_SMOKE),
    ("FOG_VOLUME_PROFILE_FLAME", FOG_VOLUME_PROFILE_FLAME),
    (
        "FOG_VOLUME_PROFILE_EXPLOSION",
        FOG_VOLUME_PROFILE_EXPLOSION,
    ),
];

// Main-pass ray-query decomposition. The runtime `DBG_DISABLE_*` bits below
// preserve the compiled shader's register allocation and isolate avoided
// execution. `RT_COMPILE_ABLATION_MASK` selects the same feature groups at
// shader compile time so glslang/the driver may eliminate dead code and lower
// register pressure. Shipping builds keep the mask at zero.
pub const RT_ABLATION_DIRECT_SHADOW: u32 = 1 << 0;
pub const RT_ABLATION_GI: u32 = 1 << 1;
pub const RT_ABLATION_REFLECTION_GLASS: u32 = 1 << 2;
pub const RT_ABLATION_ALL_RAYS: u32 = 1 << 3;
pub const RT_COMPILE_ABLATION_MASK: u32 = 0;

// Structured, mutually-exclusive renderer correctness views. These values
// ride `GpuCamera.render_debug.x`; the legacy all-bits sentinel preserves the
// launch-time `BYROREDUX_RENDER_DEBUG` categorical selectors until an operator
// explicitly chooses a named runtime mode.
pub const RENDER_DEBUG_FINAL: u32 = 0;
pub const RENDER_DEBUG_SHADOW_VISIBILITY: u32 = 1;
pub const RENDER_DEBUG_SELECTED_LIGHT: u32 = 2;
pub const RENDER_DEBUG_DIRECT_ONLY: u32 = 3;
pub const RENDER_DEBUG_INDIRECT_ONLY: u32 = 4;
pub const RENDER_DEBUG_MATERIAL_LOBE: u32 = 5;
pub const RENDER_DEBUG_COMPOSITE_TERM: u32 = 6;
pub const RENDER_DEBUG_RT_LOD: u32 = 7;
pub const RENDER_DEBUG_VOLUMETRIC_TERM: u32 = 8;
pub const RENDER_DEBUG_MODE_MAX: u32 = RENDER_DEBUG_VOLUMETRIC_TERM;
pub const RENDER_DEBUG_LEGACY_FLAGS: u32 = u32::MAX;

pub const RENDER_DEBUG_MODES: &[(&str, u32)] = &[
    ("RENDER_DEBUG_FINAL", RENDER_DEBUG_FINAL),
    (
        "RENDER_DEBUG_SHADOW_VISIBILITY",
        RENDER_DEBUG_SHADOW_VISIBILITY,
    ),
    ("RENDER_DEBUG_SELECTED_LIGHT", RENDER_DEBUG_SELECTED_LIGHT),
    ("RENDER_DEBUG_DIRECT_ONLY", RENDER_DEBUG_DIRECT_ONLY),
    ("RENDER_DEBUG_INDIRECT_ONLY", RENDER_DEBUG_INDIRECT_ONLY),
    ("RENDER_DEBUG_MATERIAL_LOBE", RENDER_DEBUG_MATERIAL_LOBE),
    ("RENDER_DEBUG_COMPOSITE_TERM", RENDER_DEBUG_COMPOSITE_TERM),
    ("RENDER_DEBUG_RT_LOD", RENDER_DEBUG_RT_LOD),
    (
        "RENDER_DEBUG_VOLUMETRIC_TERM",
        RENDER_DEBUG_VOLUMETRIC_TERM,
    ),
    ("RENDER_DEBUG_MODE_MAX", RENDER_DEBUG_MODE_MAX),
    ("RENDER_DEBUG_LEGACY_FLAGS", RENDER_DEBUG_LEGACY_FLAGS),
];

// Debug-viz bit flags packed into `jitter.z` by the renderer
// (`parse_render_debug_flags_env` + `GpuCamera` upload). Runtime-set
// via `BYROREDUX_RENDER_DEBUG=<bitmask>` env var or console for
// renderer-artifact bisection. Branches collapse to free no-ops when
// the env var is unset. Consumed by `triangle.frag` via the `#include`d
// `#define`s; this file is the single source of truth.

/// 0x1 — bypass parallax-occlusion mapping in the base-UV sampler.
pub const DBG_BYPASS_POM: u32 = 0x1;

/// 0x2 — bypass detail-map blend on the base albedo.
pub const DBG_BYPASS_DETAIL: u32 = 0x2;

/// 0x4 — visualize per-fragment world-space normal as colour.
pub const DBG_VIZ_NORMALS: u32 = 0x4;

/// 0x8 — visualize per-fragment tangent presence:
///   * green = tangent present (vertex shader fed authored or synthesized
///     data → Path 1 in `perturbNormal` fires).
///   * red = zero tangent → screen-space derivative fallback (Path 2).
///
/// Added under #783 follow-up.
pub const DBG_VIZ_TANGENT: u32 = 0x8;

/// 0x10 — skip the per-fragment normal-map perturbation entirely;
/// lighting uses the geometric vertex normal. Use to bisect whether a
/// chrome / posterization artifact originates from `perturbNormal`
/// (Path 1 or Path 2 TBN bug) or from downstream specular / ambient
/// code. Default-on path runs `perturbNormal`; this bit is the opt-out.
/// 2026-05-03 / #786 closeout reinstated the default-on behaviour after
/// the convention-swap fix at 5dde345 + the BSTriShape inline-tangent
/// decode at b63ab0c.
pub const DBG_BYPASS_NORMAL_MAP: u32 = 0x10;

/// 0x20 — RESERVED. Pre-#1035 (in the 77aa2de → 5dde345 window) this
/// bit was the opt-IN for `perturbNormal` while the default was off
/// (was named `DBG_FORCE_NORMAL_MAP`). After #786 closed (2026-05-03)
/// the default flipped back to on and the bit became a silent no-op.
/// Kept reserved so existing diagnostic scripts using
/// `BYROREDUX_RENDER_DEBUG=0x20` / `0x24` / `0x28` keep working as
/// no-ops; renamed at #1035 to make the no-op status explicit in the
/// bit catalog.
pub const DBG_RESERVED_20: u32 = 0x20;

/// 0x40 — visualize the per-entity content-class render layer driving
/// the depth-bias ladder. Tints fragments by layer:
///   * Architecture (0) → grey
///   * Clutter (1)      → cyan
///   * Actor (2)        → magenta
///   * Decal (3)        → yellow
///
/// The 2-bit layer is packed into `gpuInstance.flags` bits 4..5
/// (`INSTANCE_RENDER_LAYER_SHIFT` / `_MASK` on the Rust side).
pub const DBG_VIZ_RENDER_LAYER: u32 = 0x40;

/// 0x80 — glass IOR refraction passthru-loop diagnostic (#789
/// follow-up). Tints glass fragments by where the loop terminated:
///   * black   — IOR not allowed (thin glass, RT globally disabled, or an
///     architectural window whose portal classification remained valid).
///   * red     — IOR fired but ray escaped scene (sky fallback).
///   * yellow  — terminated on first hit, no passthru (different texture
///     from start — desk / wall / non-glass behind the surface).
///   * green   — passthru ×1, then non-self terminus (one self skip,
///     then real scene geometry).
///   * cyan    — passthru ×2 with non-self terminus (two self skips +
///     real geometry, e.g. through one stacked beaker to wall behind).
///   * magenta — interface allowance exhausted, terminus STILL glass
///     (passthru never escaped the overlapping surfaces at this tier).
pub const DBG_VIZ_GLASS_PASSTHRU: u32 = 0x80;

/// 0x100 — disable specular antialiasing (`specularAaRoughness`).
/// Every per-light + RT-reflection BRDF site widens the authored
/// `roughness` by the screen-space normal-variance kernel before
/// feeding it to GGX/Smith. Setting this bit returns to the raw
/// authored roughness so the Kaplanyan-Hoffman 2016 bug-class
/// (corrugated normal map → bright/dark stripes at distance) can be
/// A/B'd against a regression suspect that turns out to be the spec-AA
/// itself. Default-on; this bit is the opt-out.
pub const DBG_DISABLE_SPECULAR_AA: u32 = 0x100;

/// 0x200 — reserved. Formerly disabled the interior-only isotropic
/// directional-fill path. Interior and exterior directionals now share
/// the standard BRDF + RT-shadow contract, so keep the bit vacant rather
/// than renumbering the externally visible debug flags above it.
pub const DBG_RESERVED_200: u32 = 0x200;

/// 0x400 — bypass the per-vertex color modulation of albedo
/// (`albedo *= fragColor`). Bethesda bakes per-vertex lighting / AO into
/// static-geometry vertex colors; on coarsely-tessellated meshes (e.g.
/// FNV casino floor tiles — `NV_TOPS_CasLoRmMid01` carries vertex-luma
/// 0.16–1.00 over only 40 verts) that baked term interpolates across
/// large triangles into hard-edged bright/dark patches whose boundaries
/// follow the triangulation. Set this bit to confirm a "lighting error
/// only on certain polygons, at a fixed position" is the baked
/// vertex-color term rather than a dynamic / RT-side cause (Tops floor
/// diagnosis 2026-05-27). Does not affect `SOURCE_EMISSIVE` vertex mode
/// (that path routes vertex color through the emissive accumulator).
pub const DBG_BYPASS_VERTEX_COLOR: u32 = 0x400;

/// 0x800 — force ambient occlusion to 1.0 (disable both the screen-space
/// SSAO sample and the RT-AO term in `combinedAO`). Use to bisect whether
/// a hard-edged dark floor patch is AO over-darkening (vanishes with this
/// bit) versus a cast shadow / direct-light occlusion (persists). Paired
/// with `DBG_BYPASS_VERTEX_COLOR` these isolate the two most common
/// "lighting only on certain polygons" causes without touching shadows.
pub const DBG_DISABLE_AO: u32 = 0x800;

/// 0x1000 — revert point/spot lights to the pre-REND-#1451 attenuation:
/// the anti-pop-in cull window doing the ENTIRE attenuation job
/// (`atten = pow(clamp(1 − (d/R)², 0, 1), shape)`, `R = .w`). That
/// formula reads 75% at the authored radius (`d = R/2`) — the bright
/// near-zone ring (Lonesome Road / Ulysses Temple). Default-off path
/// now uses the OpenMW-style two-term model: a physical near-zone
/// falloff keyed to the AUTHORED radius (`knee = dofParams.z × .w`)
/// MULTIPLIED by a soft cull window that fades full→zero from the
/// authored radius out to `.w`. Set this bit to A/B the new model
/// against the legacy one in the same live session (no rebuild) while
/// running the REND-#1451 controlled bench. Also settable via the
/// `light.atten legacy on|off` console command (routes through the
/// `LightTuning` resource → `VulkanContext::light_atten_legacy`).
pub const DBG_LEGACY_LIGHT_ATTEN: u32 = 0x1000;

/// 0x2000 — disable multi-scatter energy compensation
/// (`multiScatterEnergyCompensation`, Fdez-Agüera 2019 / Filament). The
/// default-on path multiplies the single-scatter Cook-Torrance specular
/// lobe by `1 + F0·(1/Ess − 1)` to restore the energy lost to microfacet
/// masking as roughness rises — without it, rough conductors (brushed
/// steel, satin, cookware) progressively darken. The factor is a no-op at
/// low roughness (`Ess → 1`), so it cannot shift the RT reflection
/// roughness gate. Set this bit to A/B the compensated rough metal
/// against the legacy single-scatter look in one live session.
pub const DBG_DISABLE_MULTISCATTER: u32 = 0x2000;

/// 0x4000 — disable the SVGF spatial à-trous wavelet pass
/// (`svgf_atrous.comp`, Schied 2017 §4.3). The default-on path runs the
/// variance-guided edge-stopping wavelet filter after temporal
/// accumulation to remove the per-pixel GI variance the temporal pass
/// leaves behind (the noisy / slow-moiré floor). Setting this bit turns
/// every à-trous iteration into a pass-through copy, so the composite
/// samples the raw temporal-only result — the pre-Phase-4 look — for live
/// A/B in one session.
pub const DBG_DISABLE_ATROUS: u32 = 0x4000;

/// 0x8000 — disable ReSTIR-DI reservoir reuse for a trustworthy raw-direct
/// ablation. In the default production build (`ENABLE_LEGACY_WRS == 0`) this
/// keeps the current-frame streaming-RIS reservoir and its final visibility
/// sample, but disables temporal selection reuse, spatial neighbour reuse,
/// and the direct-radiance EMA. The result is the noisy one-frame direct-light
/// estimator needed to separate transport defects from reuse artifacts.
///
/// In an evaluation build with `ENABLE_LEGACY_WRS == 1`, the same bit still
/// selects the legacy 16-slot WRS arm for historical A/B captures. It is never
/// a no-op: changing its production meaning silently would invalidate the
/// fixed-camera lighting ladder.
pub const DBG_DISABLE_RESTIR: u32 = 0x8000;

/// 0x10000 — disable ReSTIR-DI **spatial** reservoir reuse (ReSTIR "P2",
/// Bitterli 2020 §5) while leaving the temporal reuse (`DBG_DISABLE_RESTIR`
/// path) active. The default-on path samples a small disk of neighbour
/// reservoirs from the *previous* frame's buffer around the reprojected
/// pixel, re-evaluates each neighbour's selected light against the **current**
/// surface (target pdf p̂), and combines them with the same 1/M streaming-RIS
/// estimator the temporal path uses — so a freshly disoccluded or fast-moving
/// pixel inherits many effective samples from its neighbourhood instead of
/// restarting from a single noisy frame. It also seeds the soft-shadow colour
/// EMA from valid neighbours on disocclusion (where temporal reprojection
/// fails), which is what visibly removes the "convergence resets on camera
/// motion" restart noise. Set this bit to A/B temporal-only ReSTIR against the
/// full spatiotemporal path in one live session.
pub const DBG_DISABLE_SPATIAL: u32 = 0x10000;

/// 0x20000 — #1874 diagnostic: visualise the per-fragment screen-space motion
/// vector (`outMotion`, the G-buffer velocity SVGF + TAA reproject with) as
/// colour, so the "ghosted diagonal double-image" can be root-caused live
/// without a RenderDoc capture. Encoding: `rg = 0.5 + motion.xy * scale`,
/// `b = 0.5` — a static camera reads flat grey `(0.5, 0.5, 0.5)` everywhere.
///
/// The decisive read for issue #1874's hypothesis H1 (a *spatially-uniform*
/// bad motion vector shared by SVGF and TAA): under this view a real camera
/// translation shows motion that **varies with depth** (near geometry tints
/// harder than far — parallax), whereas the suspected fault paints the **whole
/// screen one uniform non-grey tint** (a post-projection screen shift with no
/// depth dependence — a stale/jittered `prevViewProj`, not real motion). Park
/// the camera on the artifact and set this bit: uniform tint ⇒ camera-level
/// (`prevViewProj`/origin), depth-varying-but-localised-to-a-body tint ⇒
/// skinning. Diagnostic-only — gated entirely behind the debug bit, no effect
/// on normal rendering.
pub const DBG_VIZ_MOTION: u32 = 0x20000;

/// 0x40000 — disable ReSTIR-DI **temporal** reservoir and radiance-history
/// reuse while leaving current-frame sampling and (unless separately disabled
/// by [`DBG_DISABLE_SPATIAL`]) previous-frame spatial-neighbour reuse active.
/// This separates the two reuse dimensions for controlled evaluation:
///
/// - default: temporal + spatial reuse;
/// - `DBG_DISABLE_SPATIAL`: temporal-only;
/// - `DBG_DISABLE_TEMPORAL`: spatial-only;
/// - both bits: current-frame reservoir only.
///
/// Unlike `DBG_DISABLE_RESTIR`, this never selects the compile-time-gated
/// legacy WRS implementation. It keeps the same ReSTIR estimator and only
/// removes the centre-pixel temporal candidate plus its colour EMA history.
pub const DBG_DISABLE_TEMPORAL: u32 = 0x40000;

/// 0x80000 — display the fragment shader's resolved indirect-light signal
/// directly, before SVGF history and before multiplication by local albedo in
/// composite. This distinguishes "GI rays returned darkness" from
/// "denoising/compositing/exposure buried valid indirect energy".
pub const DBG_VIZ_RAW_INDIRECT: u32 = 0x80000;

/// 0x100000 — display final raster material classification. Opaque surfaces
/// are grey, alpha-tested surfaces green, alpha-blended surfaces red, and
/// glass blue. The diagnostic writes alpha=1 so blend-state membership is
/// visible as a solid classification rather than being obscured by the very
/// transparency defect under investigation.
pub const DBG_VIZ_MATERIAL_STATE: u32 = 0x100000;

/// 0x200000 — display only the stochastic ray-traced diffuse GI bounce,
/// excluding authored cell ambient, AO, reflections, direct light, and SVGF.
/// This is the decisive probe for whether the GI ray estimator contributes
/// energy in a real-content scene.
pub const DBG_VIZ_GI_BOUNCE: u32 = 0x200000;

/// 0x400000 — display the projection jitter and FSR reset contract as a
/// uniform diagnostic colour. Red/green encode the exact render-pixel jitter
/// mapped from `[-0.5, 0.5]` to `[0, 1]`; blue is 1 while an FSR history reset
/// is pending and 0 otherwise. This makes projection/dispatch phase drift and
/// forgotten camera-cut resets visible without a RenderDoc capture.
pub const DBG_VIZ_FSR_TEMPORAL: u32 = 0x400000;

/// 0x800000 — #2218 diagnostic: bisect which shading term first goes
/// non-finite (NaN/Inf). FO3 Megaton's exterior geometry saturates to pure
/// white and survives a 42x exposure crush untouched — a value that ignores
/// a 42x reduction is non-finite, not merely bright (`ACES(Inf) → 1.0`;
/// `NaN` propagates to white through tone mapping). Checks, upstream to
/// downstream: `indirect` (the raw stochastic GI bounce, pre-ambient/AO) →
/// `indirectLight` (adds authored ambient + AO) → `directLight` (the
/// ReSTIR-shaded, shadow-ray-gated direct term + emissive) — the terms that
/// feed the final composite. A fault reports at the earliest term it
/// reaches, since a non-finite value there also poisons every downstream sum.
///   magenta = `indirect` (GI bounce) non-finite
///   yellow  = `indirectLight` (ambient + GI + AO) non-finite
///   red     = `directLight` (direct + shadow + emissive) non-finite
///   green   = every checked term finite at this fragment
pub const DBG_VIZ_NONFINITE: u32 = 0x800000;

/// 0x1000000 — display the scale-aware RT ray-origin offset magnitude as
/// false colour. Blue is approximately 2^-16 world units (the near-origin
/// additive fallback); red is approximately 2^1 world units at large float
/// exponents. A surface that still shows uniform speckle while this view is
/// warm-coloured is not suffering from an undersized self-hit epsilon.
pub const DBG_VIZ_SHADOW_OFFSET: u32 = 0x1000000;

/// 0x2000000 — display the angle between the actual triangle-plane normal
/// used by RT visibility and the fully resolved shading normal. Green is
/// aligned; red is 90 degrees apart. A band localised to warm/red geometry
/// is the shadow-terminator class, not floating-point self-intersection.
pub const DBG_VIZ_NORMAL_DIVERGENCE: u32 = 0x2000000;

/// 0x4000000 — display only the resolved direct-light attachment, including
/// emissive but excluding authored ambient/GI, SVGF, and albedo remodulation.
/// Pair with [`DBG_DISABLE_RESTIR`] for the raw one-frame direct-transport rung
/// of the lighting ablation ladder.
pub const DBG_VIZ_DIRECT: u32 = 0x4000000;

/// 0x8000000 — skip primary direct-light shadow queries while retaining the
/// unshadowed light estimator. GI/reflection hit lighting can still issue its
/// own visibility queries; this bit isolates the shipping direct-shadow path.
pub const DBG_DISABLE_DIRECT_SHADOWS: u32 = 0x8000000;

/// 0x10000000 — skip the bounded diffuse GI path. Authored ambient, direct
/// lighting, reflections, SSAO, and every non-GI pass remain active.
pub const DBG_DISABLE_GI_RAYS: u32 = 0x10000000;

/// 0x20000000 — skip main-pass window portal, fire-refraction, glass IOR, and
/// glossy/metal reflection queries. Glass keeps its zero-ray Fresnel fallback.
pub const DBG_DISABLE_REFLECTION_GLASS_RAYS: u32 = 0x20000000;

/// 0x40000000 — skip every ray-query path in `triangle.frag`. TLAS build and
/// ray-query work in other pipelines remain intact, so baseline→this bit is
/// the main-pass ray budget rather than a whole-frame RT-off measurement.
pub const DBG_DISABLE_ALL_MAIN_RAYS: u32 = 0x40000000;

/// 0x80000000 — false-colour the final ReSTIR-selected light index. A stable
/// integer hash maps each SSBO index to a distinct colour; black means the
/// fragment had no valid reservoir selection. This makes a selected-index /
/// shadow-ray payload mismatch visible without conflating it with BRDF,
/// visibility, denoising, or composite energy.
pub const DBG_VIZ_SELECTED_LIGHT: u32 = 0x80000000;

/// Compound selector (not a new bit): false-colour the material shading lobe
/// while preserving every legacy one-bit debug flag. Enable it by setting both
/// `DBG_VIZ_MATERIAL_STATE` and `DBG_VIZ_SELECTED_LIGHT`; the shader checks
/// this exact combination before either constituent view.
pub const DBG_VIZ_MATERIAL_LOBES: u32 = DBG_VIZ_MATERIAL_STATE | DBG_VIZ_SELECTED_LIGHT;

/// Compound selector (not a new bit): false-colour the continuous `rtLOD`
/// value used by reflection/GI and portal-cost gates. Glass identity and thick
/// transmission are deliberately not distance-gated. Enable both bits.
/// This is the measurement surface required before retuning RT_LOD_SCALE.
pub const DBG_VIZ_RT_LOD: u32 = DBG_VIZ_MATERIAL_STATE | DBG_VIZ_GI_BOUNCE;

/// Compound selector (not a new bit): display the final selected direct-light
/// visibility before BRDF, ReSTIR weight, temporal accumulation, indirect,
/// composite, exposure, or tone mapping. Greyscale is luminance of the
/// material-aware RGB transmittance (black=blocked, white=fully visible);
/// magenta means no valid reservoir sample/ray existed for the fragment, so
/// "occluded" can never be confused with "nothing selected".
pub const DBG_VIZ_SHADOW_VISIBILITY: u32 = DBG_VIZ_SELECTED_LIGHT | DBG_VIZ_DIRECT;

/// Debug-viz views that are correctness oracles, keyed by **any one bit**:
/// if the flag word carries any of these, raw output is required.
///
/// #2978 — this catalog plus [`DBG_VIZ_RAW_OUTPUT_ALL`] is the single source
/// of truth for the raw-output policy. `build.rs` folds it into the
/// `DBG_VIZ_RAW_OUTPUT_ANY_MASK` / `DBG_VIZ_REQUIRES_RAW_OUTPUT(flags)`
/// header emit that `composite.frag` and `presentation.frag` consume, and
/// `shader_constants::debug_viz_requires_raw_output` evaluates the same two
/// catalogs in Rust. Before that the policy was hand-written once in Rust and
/// twice in GLSL, held together only by a four-literal `source.contains`
/// subset check — so a fifth view could land in Rust while both shaders went
/// on tone-mapping the oracle, with the whole suite green.
///
/// Note the compound views need no entry here: `DBG_VIZ_SHADOW_VISIBILITY`
/// and `DBG_VIZ_MATERIAL_LOBES` both contain `DBG_VIZ_SELECTED_LIGHT`, so
/// they already match on that bit.
pub const DBG_VIZ_RAW_OUTPUT_ANY: &[(&str, u32)] = &[
    ("DBG_VIZ_SELECTED_LIGHT", DBG_VIZ_SELECTED_LIGHT),
    ("DBG_VIZ_DIRECT", DBG_VIZ_DIRECT),
    ("DBG_VIZ_RAW_INDIRECT", DBG_VIZ_RAW_INDIRECT),
];

/// Correctness-oracle views keyed by an **exact compound**: every bit in the
/// mask must be set, because the constituent bits individually select a
/// different (non-oracle) view. `DBG_VIZ_RT_LOD` is `MATERIAL_STATE |
/// GI_BOUNCE`, and neither of those alone requires raw output.
///
/// Companion to [`DBG_VIZ_RAW_OUTPUT_ANY`]; see that constant for the #2978
/// rationale.
pub const DBG_VIZ_RAW_OUTPUT_ALL: &[(&str, u32)] = &[("DBG_VIZ_RT_LOD", DBG_VIZ_RT_LOD)];

/// Folded [`DBG_VIZ_RAW_OUTPUT_ANY`] mask. A `const fn` rather than a
/// `pub const DBG_…: u32` so it stays out of the `DBG_*` *bit* census that
/// `dbg_bits_catalog_covers_every_dbg_constant` counts — it is derived from
/// bits, not one of them.
pub const fn dbg_viz_raw_output_any_mask() -> u32 {
    let mut mask = 0u32;
    let mut i = 0;
    while i < DBG_VIZ_RAW_OUTPUT_ANY.len() {
        mask |= DBG_VIZ_RAW_OUTPUT_ANY[i].1;
        i += 1;
    }
    mask
}

/// Single source of truth for every `DBG_*` debug-viz bit, in emit order.
/// Both `build.rs` (GLSL header emit) and `shader_constants.rs`'s test
/// module (`generated_header_contains_all_defines` value-pin,
/// `triangle_frag_dbg_bits_not_redeclared` no-shadow guard, and
/// `dbg_bits_catalog_covers_every_dbg_constant`) drive off this single list,
/// so a new `DBG_*` constant can no longer land covered by only one (or
/// zero) of those three contracts. Pre-#1860 `build.rs` hand-emitted each
/// `DBG_*` `writeln!` separately and this catalog (then test-only, living in
/// `shader_constants.rs`) had drifted to 13 of 18 constants — the 5 newest
/// bits (`DBG_DISABLE_MULTISCATTER`/`ATROUS`/`RESTIR`/`SPATIAL`,
/// `DBG_VIZ_MOTION`) bypassed both the value-pin and the no-redeclare guard
/// silently. See #1482 (original catalog fix) / #1860 (this fix, moving the
/// catalog here so `build.rs` can drive its emit from it too).
pub const DBG_BITS: &[(&str, u32)] = &[
    ("DBG_BYPASS_POM", DBG_BYPASS_POM),
    ("DBG_BYPASS_DETAIL", DBG_BYPASS_DETAIL),
    ("DBG_VIZ_NORMALS", DBG_VIZ_NORMALS),
    ("DBG_VIZ_TANGENT", DBG_VIZ_TANGENT),
    ("DBG_BYPASS_NORMAL_MAP", DBG_BYPASS_NORMAL_MAP),
    ("DBG_RESERVED_20", DBG_RESERVED_20),
    ("DBG_VIZ_RENDER_LAYER", DBG_VIZ_RENDER_LAYER),
    ("DBG_VIZ_GLASS_PASSTHRU", DBG_VIZ_GLASS_PASSTHRU),
    ("DBG_DISABLE_SPECULAR_AA", DBG_DISABLE_SPECULAR_AA),
    ("DBG_RESERVED_200", DBG_RESERVED_200),
    ("DBG_BYPASS_VERTEX_COLOR", DBG_BYPASS_VERTEX_COLOR),
    ("DBG_DISABLE_AO", DBG_DISABLE_AO),
    ("DBG_LEGACY_LIGHT_ATTEN", DBG_LEGACY_LIGHT_ATTEN),
    ("DBG_DISABLE_MULTISCATTER", DBG_DISABLE_MULTISCATTER),
    ("DBG_DISABLE_ATROUS", DBG_DISABLE_ATROUS),
    ("DBG_DISABLE_RESTIR", DBG_DISABLE_RESTIR),
    ("DBG_DISABLE_SPATIAL", DBG_DISABLE_SPATIAL),
    ("DBG_VIZ_MOTION", DBG_VIZ_MOTION),
    ("DBG_DISABLE_TEMPORAL", DBG_DISABLE_TEMPORAL),
    ("DBG_VIZ_RAW_INDIRECT", DBG_VIZ_RAW_INDIRECT),
    ("DBG_VIZ_MATERIAL_STATE", DBG_VIZ_MATERIAL_STATE),
    ("DBG_VIZ_GI_BOUNCE", DBG_VIZ_GI_BOUNCE),
    ("DBG_VIZ_FSR_TEMPORAL", DBG_VIZ_FSR_TEMPORAL),
    ("DBG_VIZ_NONFINITE", DBG_VIZ_NONFINITE),
    ("DBG_VIZ_SHADOW_OFFSET", DBG_VIZ_SHADOW_OFFSET),
    ("DBG_VIZ_NORMAL_DIVERGENCE", DBG_VIZ_NORMAL_DIVERGENCE),
    ("DBG_VIZ_DIRECT", DBG_VIZ_DIRECT),
    ("DBG_DISABLE_DIRECT_SHADOWS", DBG_DISABLE_DIRECT_SHADOWS),
    ("DBG_DISABLE_GI_RAYS", DBG_DISABLE_GI_RAYS),
    (
        "DBG_DISABLE_REFLECTION_GLASS_RAYS",
        DBG_DISABLE_REFLECTION_GLASS_RAYS,
    ),
    ("DBG_DISABLE_ALL_MAIN_RAYS", DBG_DISABLE_ALL_MAIN_RAYS),
    ("DBG_VIZ_SELECTED_LIGHT", DBG_VIZ_SELECTED_LIGHT),
    ("DBG_VIZ_MATERIAL_LOBES", DBG_VIZ_MATERIAL_LOBES),
    ("DBG_VIZ_RT_LOD", DBG_VIZ_RT_LOD),
    ("DBG_VIZ_SHADOW_VISIBILITY", DBG_VIZ_SHADOW_VISIBILITY),
];

/// #1799 / PERF-D5-NEW-01 — compile-time gate for the legacy 16-slot WRS
/// reservoir arrays (`resLight[16]` / `resWSel[16]`) that `DBG_DISABLE_RESTIR`
/// A/Bs against. `DBG_DISABLE_RESTIR` is a RUNTIME bit read from a uniform, so
/// even on the ~100% of production frames that take the ReSTIR path and never
/// touch those arrays, the compiler still had to budget their per-invocation
/// register / local-memory footprint — the declaration + init loop ran
/// unconditionally, ahead of the runtime `useRestir` branch that gated
/// everything else about them. glslangValidator's preprocessor, unlike the
/// runtime branch, can actually eliminate dead code — but only if the
/// legacy-WRS source text is behind a `#if`, not an `if` on a uniform value.
///
/// `0` (default): the legacy WRS arm — declarations, streaming writes, and
/// pass-2 shadow-ray reads — is preprocessed OUT of `triangle.frag` entirely.
/// `DBG_DISABLE_RESTIR` then disables all history/neighbour reuse while
/// retaining the current-frame reservoir, so the diagnostic remains useful
/// without reintroducing the legacy array footprint.
///
/// `1`: restores the pre-fix behavior verbatim — the legacy arrays exist and
/// `DBG_DISABLE_RESTIR` again live-toggles between the two paths at runtime.
/// Flip this to `1` and recompile `triangle.frag` to A/B; per the mechanism
/// #1758 established for `SKIN_WORKGROUP_SIZE`, that A/B now costs a shader
/// recompile instead of a per-frame register tax on every production build.
pub const ENABLE_LEGACY_WRS: u32 = 0;
