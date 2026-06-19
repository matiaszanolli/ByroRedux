pub mod acceleration;
pub mod allocator;
pub mod bloom;
pub mod buffer;
pub mod caustic;
pub mod composite;
pub mod compute;
pub mod context;
pub mod dds;
pub mod debug;
pub mod descriptors;
pub mod device;
pub mod egui_pass;
pub mod gbuffer;
pub mod gpu_timers;
pub mod instance;
pub mod material;
pub mod pipeline;
pub mod reflect;
pub mod restir;
pub mod scene_buffer;
pub mod skin_compute;
pub mod ssao;
pub mod surface;
pub mod svgf;
pub mod swapchain;
pub mod sync;
pub mod taa;
pub mod texture;
pub mod volumetrics;
pub mod water;
pub mod water_caustic;

use allocator::SharedAllocator;

/// Bundle of the four Vulkan handles every one-time GPU upload needs.
///
/// Groups `device`, `allocator`, `queue`, and `command_pool` — the handles
/// that travel together through the texture/mesh upload helpers — so those
/// helpers stay under the argument-count lint without changing behaviour.
#[derive(Clone, Copy)]
pub struct GpuUploadCtx<'a> {
    /// Logical device the upload records against.
    pub device: &'a ash::Device,
    /// Shared GPU allocator backing staging + device-local buffers.
    pub allocator: &'a SharedAllocator,
    /// Queue the one-time command buffer is submitted on.
    pub queue: &'a std::sync::Mutex<ash::vk::Queue>,
    /// Command pool the one-time command buffer is allocated from.
    pub command_pool: ash::vk::CommandPool,
}
