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
pub const MAX_LIGHTS_PER_CLUSTER: u32 = 32;

// Vertex layout (global SSBO)
pub const VERTEX_STRIDE_FLOATS: u32 = 25;
pub const VERTEX_UV_OFFSET_FLOATS: u32 = 9;

// Skinning — see `byroredux_core::ecs::components::skinned_mesh::MAX_BONES_PER_MESH`
// for the vanilla-content survey that fixes this ceiling at 144 (FO76 prewardress = 133).
pub const MAX_BONES_PER_MESH: u32 = 144;

// Glass / IOR ray budget
pub const GLASS_RAY_BUDGET: u32 = 8192;
pub const GLASS_RAY_COST: u32 = 4;

// Caustic accumulation
pub const CAUSTIC_FIXED_SCALE: f32 = 65536.0;

// Compute workgroup sizes (bloom, volumetrics, SSAO, TAA)
pub const WORKGROUP_X: u32 = 8;
pub const WORKGROUP_Y: u32 = 8;
pub const WORKGROUP_Z: u32 = 8;

// Clustered light culling thread count (one warp/wavefront wide).
// Mirrored in `cluster_cull.comp` as `THREADS_PER_CLUSTER`.
pub const THREADS_PER_CLUSTER: u32 = 32;

// M58 — bloom contribution coefficient. 0.15 (≈4× the Frostbite
// SIGGRAPH 2015 default of 0.04) compensates for Bethesda content
// being LDR-authored: emissive surfaces sit in the 0–1 monitor-space
// range rather than HDR cd/m², so a Frostbite-default intensity reads
// as essentially-invisible. Hand-tuned downward from 0.20 on
// Prospector saloon (sun-lit windows + chandelier globes were
// producing halos that bled too far across walls); 0.15 keeps
// emissives obviously bloomed without flooding dim surfaces.
// Consumed by `composite.frag` via the `#include`d `#define`; mirrored
// here so Rust-side `bloom::DEFAULT_BLOOM_INTENSITY` stays in lockstep.
// See `feedback_color_space.md` for why we don't HDR-boost emissives
// globally instead.
pub const BLOOM_INTENSITY: f32 = 0.15;

// M55 — volumetric far plane. Must match `volumetrics::DEFAULT_VOLUME_FAR`
// (Rust side) and the `params.volume_extent.x` value passed to the
// injection compute pass; otherwise the slice→view-distance mapping
// disagrees and fog appears compressed or stretched. With Phase 3
// pre-integration the per-fragment cost is now ONE sampler3D tap, so
// no step-count dial is needed in `composite.frag` — quality scales
// with the froxel resolution and dt set on the host. Consumed by
// `composite.frag` (slice math) and `volumetrics_integrate.comp` (dt =
// VOLUME_FAR / FROXEL_DEPTH).
pub const VOLUME_FAR: f32 = 200.0;

// Water motion-kind enum (WATR-driven, mapped per-WATR record).
// Lockstep with `water.frag` and `byroredux/src/cell_loader/water.rs`.
pub const WATER_CALM: u32 = 0;
pub const WATER_RIVER: u32 = 1;
pub const WATER_RAPIDS: u32 = 2;
pub const WATER_WATERFALL: u32 = 3;

// Debug-viz bit flags — runtime-set via console for renderer bisects.
// Lockstep with `triangle.frag::DBG_*` constants (lines ~743-829).
pub const DBG_BYPASS_POM: u32 = 0x1;
pub const DBG_BYPASS_DETAIL: u32 = 0x2;
pub const DBG_VIZ_NORMALS: u32 = 0x4;
pub const DBG_VIZ_TANGENT: u32 = 0x8;
pub const DBG_BYPASS_NORMAL_MAP: u32 = 0x10;
pub const DBG_RESERVED_20: u32 = 0x20;
pub const DBG_VIZ_RENDER_LAYER: u32 = 0x40;
pub const DBG_VIZ_GLASS_PASSTHRU: u32 = 0x80;
pub const DBG_DISABLE_SPECULAR_AA: u32 = 0x100;
pub const DBG_DISABLE_HALF_LAMBERT_FILL: u32 = 0x200;
