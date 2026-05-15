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

// Skinning
pub const MAX_BONES_PER_MESH: u32 = 128;

// Glass / IOR ray budget
pub const GLASS_RAY_BUDGET: u32 = 8192;
pub const GLASS_RAY_COST: u32 = 4;

// Caustic accumulation
pub const CAUSTIC_FIXED_SCALE: f32 = 65536.0;

// Compute workgroup sizes (bloom, volumetrics, SSAO, TAA)
pub const WORKGROUP_X: u32 = 8;
pub const WORKGROUP_Y: u32 = 8;
pub const WORKGROUP_Z: u32 = 8;
