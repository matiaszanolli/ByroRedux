//! 1×1 placeholder images that keep a descriptor binding pointing at live
//! memory when an *optional* pass fails to initialise or re-create.
//!
//! # Why this exists
//!
//! SSAO and the water-caustic accumulator are both optional: their creation
//! is allowed to fail (VRAM pressure during a drag-resize with a large cell
//! loaded is the realistic trigger), and the engine is meant to degrade
//! gracefully. But the descriptor sets that reference them are written once
//! and bound unconditionally every frame:
//!
//! * scene set 1 / binding 7 (`aoTexture`) — `triangle.frag` samples it with
//!   no gate (RL-D6-01 / #2141).
//! * `WaterPipeline` set 2 / binding 0 — `record_draw` binds set 2
//!   unconditionally and the shader issues `imageAtomicAdd` against it
//!   (RL-D6-02 / #2142).
//!
//! So "disable the pass" is not enough: on the failure arms the old images
//! are destroyed while the descriptors still name them, and the next frame
//! reads (or *writes*) freed GPU memory. Rebinding to a placeholder is what
//! actually makes the degraded state safe.
//!
//! The values are chosen to be semantically inert, not merely valid:
//! the AO placeholder is white (`1.0` = fully unoccluded, which is exactly
//! what "no SSAO" should look like), and the caustic placeholder is a
//! scratch sink whose accumulated atomics are never read back.
//!
//! Both are 1×1, so the whole mechanism costs a few hundred bytes of VRAM
//! and is created once at context init — never on the failure path itself,
//! where allocating more memory is the last thing that would work.

use anyhow::{Context, Result};
use ash::vk;

use super::allocator::SharedAllocator;
use super::image::{GpuImage, GpuImageDesc};

/// Where a placeholder image ends up after its one-time clear, and the
/// stage/access its first real use will read or write it from. Bundled so
/// `clear_and_transition` stays under the argument-count lint.
struct FinalState {
    layout: vk::ImageLayout,
    stage: vk::PipelineStageFlags,
    access: vk::AccessFlags,
}

/// A 1×1 image + view (+ optional sampler) held for the lifetime of the
/// [`VulkanContext`](super::context::VulkanContext).
///
/// Destroy explicitly via [`destroy`](Self::destroy) — like the crate's
/// other GPU wrappers, dropping without it leaks.
pub struct PlaceholderImage {
    /// #3860 — the image/view/allocation triple. The sampler stays a separate
    /// field: `GpuImage` deliberately does not own samplers, which have their
    /// own lifetime and are shared across images at several sites.
    pub gpu: GpuImage,
    /// `vk::Sampler::null()` for storage-only placeholders, which are
    /// bound as `STORAGE_IMAGE` and take no sampler.
    pub sampler: vk::Sampler,
}

impl PlaceholderImage {
    /// 1×1 `R8_UNORM` white, left in `SHADER_READ_ONLY_OPTIMAL`, with a
    /// sampler — the "AO = 1.0, nothing is occluded" stand-in for scene
    /// binding 7. Format matches `SsaoPipeline`'s real AO images so the
    /// descriptor write is identical in shape.
    pub fn new_white_ao(
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
    ) -> Result<Self> {
        let mut p = Self::create(
            device,
            allocator,
            vk::Format::R8_UNORM,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            "placeholder_ao",
        )?;

        // White = 1.0 in R8_UNORM. `cmd_clear_color_image` takes the value
        // in the image's own numeric space, so float32[0] = 1.0 is right
        // for UNORM (same call SsaoPipeline::initialize_ao_images makes).
        if let Err(e) = Self::clear_and_transition(
            device,
            queue,
            pool,
            p.gpu.image,
            vk::ClearColorValue {
                float32: [1.0, 0.0, 0.0, 0.0],
            },
            FinalState {
                layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                stage: vk::PipelineStageFlags::FRAGMENT_SHADER,
                access: vk::AccessFlags::SHADER_READ,
            },
        ) {
            // SAFETY: `p`'s handles were created just above by this device
            // and have never been referenced by a command buffer that
            // reached the GPU (the one-time submit either failed to record
            // or waited on its own fence before returning).
            unsafe { p.destroy(device, allocator) };
            return Err(e);
        }

        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::NEAREST)
            .min_filter(vk::Filter::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE);
        // SAFETY: `device` is live; `sampler_info` is fully populated and
        // outlives the call. Handle ownership moves into `p`.
        p.sampler = match unsafe { device.create_sampler(&sampler_info, None) } {
            Ok(s) => s,
            Err(e) => {
                // SAFETY: as above — nothing has referenced these handles.
                unsafe { p.destroy(device, allocator) };
                return Err(anyhow::anyhow!(e)).context("placeholder AO sampler");
            }
        };
        Ok(p)
    }

    /// 1×1 storage image in `GENERAL`, zero-cleared — the scratch sink the
    /// water shader's `imageAtomicAdd` lands in when the real caustic
    /// accumulator is absent. `format` should match the accumulator's so
    /// the shader's declared image format still agrees.
    pub fn new_storage_sink(
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
        format: vk::Format,
    ) -> Result<Self> {
        let mut p = Self::create(
            device,
            allocator,
            format,
            vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_DST,
            "placeholder_caustic_sink",
        )?;
        if let Err(e) = Self::clear_and_transition(
            device,
            queue,
            pool,
            p.gpu.image,
            vk::ClearColorValue { uint32: [0; 4] },
            FinalState {
                layout: vk::ImageLayout::GENERAL,
                // The water draw writes it from the fragment stage.
                stage: vk::PipelineStageFlags::FRAGMENT_SHADER,
                access: vk::AccessFlags::SHADER_WRITE,
            },
        ) {
            // SAFETY: handles created just above; never reached the GPU.
            unsafe { p.destroy(device, allocator) };
            return Err(e);
        }
        Ok(p)
    }

    /// Allocate the 1×1 image + view. Leaves it in `UNDEFINED`; callers
    /// run [`clear_and_transition`](Self::clear_and_transition) next.
    /// #3860 — was ~85 lines of create → allocate → bind → view with its own
    /// three-arm cleanup; `GpuImage::create` owns that chain now.
    fn create(
        device: &ash::Device,
        allocator: &SharedAllocator,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        name: &'static str,
    ) -> Result<Self> {
        Ok(Self {
            gpu: GpuImage::create(
                device,
                allocator,
                &GpuImageDesc::color_2d(name, 1, 1, format, usage),
            )?,
            sampler: vk::Sampler::null(),
        })
    }

    /// `UNDEFINED` → `TRANSFER_DST` → clear → `final_layout`, on a
    /// one-time command buffer that is waited on before returning.
    fn clear_and_transition(
        device: &ash::Device,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
        image: vk::Image,
        clear: vk::ClearColorValue,
        final_state: FinalState,
    ) -> Result<()> {
        let FinalState {
            layout: final_layout,
            stage: dst_stage,
            access: dst_access,
        } = final_state;
        let range = super::descriptors::color_subresource_single_mip();
        super::texture::with_one_time_commands(device, queue, pool, |cmd| {
            // #2413 / TD2-116 — shared constructor for the entry half.
            // The exit half stays hand-rolled: `FinalState` is caller-chosen
            // (SHADER_READ_ONLY_OPTIMAL/SHADER_READ for the AO placeholder,
            // GENERAL/SHADER_WRITE for the caustic sink), so the fixed
            // transfer-dst→shader-read helper does not cover it.
            let to_dst = super::descriptors::image_barrier_undef_to_transfer_dst(image, 1);
            let to_final = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(dst_access)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(final_layout)
                .image(image)
                .subresource_range(range);
            // SAFETY: `cmd` is the recording one-time buffer from
            // `with_one_time_commands`; `image` is device-owned, live, and
            // in UNDEFINED (freshly created, never transitioned). The two
            // barriers bracket the clear in the same recording scope, and
            // `with_one_time_commands` waits on its fence before returning
            // so the transitions have completed on return.
            //
            // `NONE` as srcStageMask on the first barrier: UNDEFINED has no
            // prior writes to expose — the Vulkan 1.3 idiom used by the
            // sibling `SsaoPipeline::initialize_ao_images`.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::NONE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[to_dst],
                );
                device.cmd_clear_color_image(
                    cmd,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &clear,
                    &[range],
                );
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    dst_stage,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[to_final],
                );
            }
            Ok(())
        })
    }

    /// Destroy view + sampler + image and free the allocation.
    ///
    /// # Safety
    /// The device must be idle with respect to these handles — i.e. no
    /// in-flight command buffer still references them. Called from
    /// `VulkanContext::drop` after `device_wait_idle`.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        // Reverse creation order: view, then sampler, then image, then the
        // backing allocation — a view outliving its image would be a dangling
        // parent reference. #3860 — `GpuImage::destroy` covers view → image →
        // slab in that order and is idempotent, so the sampler is the only
        // handle this function still nulls by hand.
        //
        // SAFETY: the caller guarantees (see this fn's `# Safety`) that the
        // device is idle with respect to these handles. The sampler was
        // created by this `device` and is nulled as it is destroyed, so a
        // second call is a no-op — `vkDestroy*` on `VK_NULL_HANDLE` is always
        // valid per the Vulkan spec.
        unsafe {
            if self.sampler != vk::Sampler::null() {
                device.destroy_sampler(self.sampler, None);
                self.sampler = vk::Sampler::null();
            }
        }
        self.gpu.destroy(device, allocator);
    }
}
