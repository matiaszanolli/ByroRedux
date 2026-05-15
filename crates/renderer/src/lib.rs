pub(crate) mod deferred_destroy;
pub mod mesh;
pub mod shader_constants;
pub mod texture_registry;
pub mod vertex;
pub mod vulkan;

pub use mesh::{cube_vertices, quad_vertices, triangle_vertices, MeshRegistry};
pub use texture_registry::TextureRegistry;
pub use vertex::Vertex;
pub use vulkan::context::{
    DrawCommand, FrameTimings, ScreenshotHandle, SkyDalcCube, SkyParams, VulkanContext,
};
pub use vulkan::material::{GpuMaterial, MaterialTable};
pub use vulkan::scene_buffer::{GpuLight, MATERIAL_KIND_EFFECT_SHADER, MATERIAL_KIND_GLASS};
