/// Early-return from a pass constructor that is building a `partial` value
/// field by field: on `Err`, tear the partial state down in reverse creation
/// order and return the error (#5594). Twelve constructors each defined this
/// macro locally (identical but for the name of the partial and a SAFETY
/// comment); a change to the partial-teardown contract was a twelve-site edit.
///
/// `$partial.destroy($device, $allocator)` is called with whatever the
/// constructor has created so far. Defined before the `mod` declarations so
/// it is in textual scope for every pass module below.
macro_rules! try_or_cleanup {
    ($partial:ident, $device:expr, $allocator:expr, $expr:expr) => {
        match $expr {
            Ok(v) => v,
            Err(e) => {
                // SAFETY: `$partial` holds only handles created by this device
                // earlier in the calling initializer; on this error path none
                // has been bound to an in-flight command buffer yet, so tearing
                // them down in reverse creation order is sound.
                unsafe { $partial.destroy($device, $allocator) };
                return Err(e.into());
            }
        }
    };
}

pub mod acceleration;
pub mod allocator;
pub mod bloom;
pub mod buffer;
pub mod caustic;
pub mod cloud_noise;
pub mod composite;
pub mod compute;
pub mod context;
pub mod dds;
pub mod debug;
pub mod descriptors;
pub mod device;
pub mod egui_pass;
pub mod exposure;
pub mod exposure_meter;
pub mod frame_upscaler;
pub mod gbuffer;
pub mod gpu_timers;
pub mod groundcover;
pub mod groundcover_models;
pub(crate) mod groundcover_stats;
pub mod groundcover_bench;
pub mod image;
pub mod instance;
pub mod material;
pub mod morph_compute;
pub mod pipeline;
pub mod placeholder;
pub mod presentation;
pub mod reflect;
pub mod render_debug;
pub mod restir;
pub mod scene_buffer;
pub mod skin_compute;
pub mod ssao;
pub mod surface;
pub mod sky_cube;
pub mod sky_dome;
pub mod svgf;
pub mod swapchain;
pub mod sync;
pub mod taa;
pub mod texture;
pub mod upscaling;
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
