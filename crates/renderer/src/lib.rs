// #1904 — every `unsafe {}` block in this crate now carries a `SAFETY:`
// comment stating the ash precondition it upholds (device live, handles
// created by this device, not in use by an in-flight command buffer, pointers
// valid for the call). The audit suggested `allow` at the crate root with
// per-file `deny` as files were cleaned; since that sweep cleaned the *whole*
// crate at once (verified: `cargo clippy -- -W clippy::undocumented_unsafe_blocks`
// reports zero), we go straight to the end-state and `deny` crate-wide so any
// new undocumented block fails `cargo clippy`. This is a Clippy tool-lint, so
// it is inert under plain `cargo build` / `cargo test` and only gates clippy.
#![deny(clippy::undocumented_unsafe_blocks)]

pub(crate) mod deferred_destroy;
pub mod mesh;
pub mod shader_constants;
pub mod texture_registry;
pub mod vertex;
pub mod vulkan;

pub use mesh::{
    box_vertices_colored, cube_vertices, quad_vertices, triangle_vertices, uv_sphere,
    MeshRegistry,
};
pub use texture_registry::TextureRegistry;
pub use vertex::Vertex;
pub use vulkan::context::{
    DofView, DrawCommand, FrameTimings, ScreenshotHandle, SkyDalcCube, SkyParams, VulkanContext,
};
pub use vulkan::material::{GpuMaterial, MaterialTable};
pub use vulkan::scene_buffer::{
    GpuLight, MATERIAL_KIND_EFFECT_SHADER, MATERIAL_KIND_GLASS, MATERIAL_KIND_NO_LIGHTING,
    MAX_MATERIALS,
};
