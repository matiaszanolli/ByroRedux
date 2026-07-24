//! Render-resolution scene color to output-resolution HDR reconstruction.
//!
//! The resource split in this module is deliberately useful without FSR:
//! `UpscalerMode::Taa` records a native Vulkan blit into the same output HDR
//! target that the FSR path writes. This keeps scene composition and
//! presentation decoupled, and gives FSR one explicit frame-graph slot instead
//! of letting the final composite pass silently bilinear-upscale its inputs.

use super::allocator::SharedAllocator;
use super::composite::HDR_FORMAT;
use super::descriptors::color_subresource_single_mip;
use super::exposure::EXPOSURE_FORMAT;
use super::gbuffer::MOTION_FORMAT;
use super::sync::MAX_FRAMES_IN_FLIGHT;
use super::upscaling::{fsr_motion_vector_scale, FrameExtentSet, UpscalerMode};
use anyhow::{Context, Result};
use ash::vk::{self, Handle};
use byroredux_fsr3_sys as fsr3;
use gpu_allocator::vulkan as vk_alloc;
use gpu_allocator::MemoryLocation;
use std::ffi::c_void;

/// Camera and temporal values that change for every FSR dispatch.
#[derive(Debug, Clone, Copy)]
pub struct FsrFrameParameters {
    pub jitter_offset: [f32; 2],
    pub reset: bool,
    pub frame_time_delta_ms: f32,
    pub camera_near: f32,
    pub camera_far: f32,
    pub camera_fov_angle_vertical: f32,
}

/// Engine-owned images consumed by one upscale operation.
#[derive(Debug, Clone, Copy)]
pub struct UpscaleDispatchInputs {
    pub scene_color: vk::Image,
    pub depth: vk::Image,
    pub depth_format: vk::Format,
    pub motion_vectors: vk::Image,
    pub exposure: Option<vk::Image>,
}

/// Owns the output-resolution HDR images and, in FSR mode, the SDK context.
pub struct FrameUpscaler {
    mode: UpscalerMode,
    context: Option<fsr3::Context>,
    output_images: Vec<vk::Image>,
    output_views: Vec<vk::ImageView>,
    output_allocations: Vec<Option<vk_alloc::Allocation>>,
    extents: FrameExtentSet,
    dispatched_this_frame: bool,
}

impl FrameUpscaler {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        instance: &ash::Instance,
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        mode: UpscalerMode,
        extents: FrameExtentSet,
    ) -> Result<Self> {
        let mut upscaler = Self {
            mode,
            context: None,
            output_images: Vec::new(),
            output_views: Vec::new(),
            output_allocations: Vec::new(),
            extents,
            dispatched_this_frame: false,
        };

        if let Err(error) = upscaler.create_outputs(device, allocator) {
            // SAFETY: construction has not returned, no command buffer can
            // reference the partially-created output resources.
            unsafe { upscaler.destroy(device, allocator) };
            return Err(error);
        }
        if let Err(error) = upscaler.initialize_outputs(device, queue, command_pool) {
            // SAFETY: the one-time initialization failed before this value
            // became externally visible and the queue helper fenced its submit.
            unsafe { upscaler.destroy(device, allocator) };
            return Err(error);
        }

        if matches!(mode, UpscalerMode::Fsr3(_)) {
            let create = unsafe {
                // SAFETY: all handles come from the live renderer device and
                // outlive `upscaler`; the context is destroyed before device
                // teardown after `device_wait_idle`.
                fsr3::Context::create(fsr3::VulkanCreateInfo {
                    device: device.handle().as_raw() as usize,
                    physical_device: physical_device.as_raw() as usize,
                    get_device_proc_addr: instance.fp_v1_0().get_device_proc_addr as *const ()
                        as *const c_void,
                    max_render_size: [extents.render.width, extents.render.height],
                    max_upscale_size: [extents.output.width, extents.output.height],
                    high_dynamic_range: true,
                    debug_checking: cfg!(debug_assertions),
                })
            };
            match create {
                Ok(context) => {
                    log::info!(
                        "FSR {} dispatch context active ({}x{} -> {}x{})",
                        fsr3::version()
                            .map(|version| version.to_string())
                            .unwrap_or_else(|_| "unknown".to_owned()),
                        extents.render.width,
                        extents.render.height,
                        extents.output.width,
                        extents.output.height,
                    );
                    upscaler.context = Some(context);
                }
                Err(error) => {
                    // Keep the frame graph alive through the Phase-4 native
                    // bridge. This is intentionally loud: the selected FSR
                    // mode is not silently reported as an active SDK path.
                    log::error!(
                        "FSR context creation failed: {error}; using native HDR blit fallback"
                    );
                }
            }
        }

        Ok(upscaler)
    }

    fn create_outputs(&mut self, device: &ash::Device, allocator: &SharedAllocator) -> Result<()> {
        for frame in 0..MAX_FRAMES_IN_FLIGHT {
            let info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(HDR_FORMAT)
                .extent(vk::Extent3D {
                    width: self.extents.output.width,
                    height: self.extents.output.height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(
                    vk::ImageUsageFlags::STORAGE
                        | vk::ImageUsageFlags::SAMPLED
                        | vk::ImageUsageFlags::TRANSFER_DST,
                )
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED);
            let image = unsafe {
                // SAFETY: `info` is fully initialized and the returned image
                // is stored immediately for cleanup on every later failure.
                device
                    .create_image(&info, None)
                    .context("create upscale output image")?
            };
            self.output_images.push(image);
            self.output_allocations.push(None);

            let requirements = unsafe {
                // SAFETY: `image` was just created by this device and is live.
                device.get_image_memory_requirements(image)
            };
            let allocation = allocator
                .lock()
                .expect("allocator lock poisoned")
                .allocate(&vk_alloc::AllocationCreateDesc {
                    name: &format!("upscale_output_{frame}"),
                    requirements,
                    location: MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
                })
                .context("allocate upscale output image")?;
            unsafe {
                // SAFETY: `allocation` was created from this image's exact
                // requirements and the image has not been bound before.
                device
                    .bind_image_memory(image, allocation.memory(), allocation.offset())
                    .context("bind upscale output image")?;
            }
            self.output_allocations[frame] = Some(allocation);

            let view = unsafe {
                // SAFETY: the image is live, bound, and format-compatible with
                // this single-mip 2D color view.
                device
                    .create_image_view(
                        &vk::ImageViewCreateInfo::default()
                            .image(image)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .format(HDR_FORMAT)
                            .subresource_range(color_subresource_single_mip()),
                        None,
                    )
                    .context("create upscale output view")?
            };
            self.output_views.push(view);
        }
        Ok(())
    }

    fn initialize_outputs(
        &self,
        device: &ash::Device,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
    ) -> Result<()> {
        super::texture::with_one_time_commands(device, queue, command_pool, |cmd| {
            let barriers: Vec<_> = self
                .output_images
                .iter()
                .map(|&image| {
                    vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::empty())
                        .dst_access_mask(vk::AccessFlags::SHADER_READ)
                        .old_layout(vk::ImageLayout::UNDEFINED)
                        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .image(image)
                        .subresource_range(color_subresource_single_mip())
                })
                .collect();
            unsafe {
                // SAFETY: `cmd` is recording in the queue helper; these fresh
                // images have never been submitted and are all UNDEFINED.
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::NONE,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &barriers,
                );
            }
            Ok(())
        })
        .context("initialize upscale output layouts")
    }

    pub fn output_views(&self) -> &[vk::ImageView] {
        &self.output_views
    }

    pub fn output_image(&self, frame: usize) -> vk::Image {
        self.output_images[frame]
    }

    pub fn is_fsr_dispatch_active(&self) -> bool {
        self.context.is_some()
    }

    /// Record either the real FSR dispatch or the native blit bridge.
    ///
    /// # Safety
    ///
    /// `cmd` must be recording outside a render pass. All input images must
    /// remain live through submission and start in the layouts documented by
    /// the barriers below.
    pub unsafe fn record(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        inputs: UpscaleDispatchInputs,
        fsr_frame: Option<FsrFrameParameters>,
    ) -> Result<()> {
        self.dispatched_this_frame = false;
        if self.context.is_none() {
            unsafe { self.record_native_blit(device, cmd, frame, inputs.scene_color) };
            return Ok(());
        }

        let frame_params =
            fsr_frame.context("FSR context is active but frame parameters are absent")?;
        unsafe { self.record_fsr_barriers_before(device, cmd, frame, inputs) };

        let render_size = [self.extents.render.width, self.extents.render.height];
        let output_size = [self.extents.output.width, self.extents.output.height];
        let output = self.output_images[frame];
        let context = self
            .context
            .as_mut()
            .expect("context presence checked above");
        unsafe {
            // SAFETY: boundary barriers above establish the layouts and access
            // states described to the SDK; all handles belong to this device
            // and the renderer keeps them alive through queue completion.
            context
                .dispatch(fsr3::DispatchDescription {
                    command_buffer: cmd.as_raw() as usize,
                    color: vulkan_image(
                        inputs.scene_color,
                        HDR_FORMAT,
                        vk::ImageUsageFlags::COLOR_ATTACHMENT
                            | vk::ImageUsageFlags::SAMPLED
                            | vk::ImageUsageFlags::TRANSFER_SRC,
                        render_size,
                    ),
                    depth: vulkan_image(
                        inputs.depth,
                        inputs.depth_format,
                        vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                            | vk::ImageUsageFlags::SAMPLED
                            | vk::ImageUsageFlags::TRANSFER_SRC,
                        render_size,
                    ),
                    motion_vectors: vulkan_image(
                        inputs.motion_vectors,
                        MOTION_FORMAT,
                        vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                        render_size,
                    ),
                    exposure: inputs.exposure.map(|image| {
                        vulkan_image(
                            image,
                            EXPOSURE_FORMAT,
                            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
                            [1, 1],
                        )
                    }),
                    reactive: None,
                    transparency_and_composition: None,
                    output: vulkan_image(
                        output,
                        HDR_FORMAT,
                        vk::ImageUsageFlags::STORAGE
                            | vk::ImageUsageFlags::SAMPLED
                            | vk::ImageUsageFlags::TRANSFER_DST,
                        output_size,
                    ),
                    jitter_offset: frame_params.jitter_offset,
                    motion_vector_scale: fsr_motion_vector_scale(self.extents.render),
                    render_size,
                    upscale_size: output_size,
                    frame_time_delta_ms: frame_params.frame_time_delta_ms.max(0.001),
                    pre_exposure: 1.0,
                    reset: frame_params.reset,
                    camera_near: frame_params.camera_near,
                    camera_far: frame_params.camera_far,
                    camera_fov_angle_vertical: frame_params.camera_fov_angle_vertical,
                    view_space_to_meters_factor: 1.0,
                    enable_sharpening: false,
                    sharpness: 0.0,
                })
                .context("record FSR upscale dispatch")?;
        }
        unsafe { self.record_fsr_barriers_after(device, cmd, frame, inputs.depth) };
        self.dispatched_this_frame = true;
        Ok(())
    }

    unsafe fn record_native_blit(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        scene_color: vk::Image,
    ) {
        let range = color_subresource_single_mip();
        let output = self.output_images[frame];
        let before = [
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .image(scene_color)
                .subresource_range(range),
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_READ)
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .image(output)
                .subresource_range(range),
        ];
        unsafe {
            // SAFETY: caller guarantees `cmd` is recording outside a render
            // pass; scene composition left `scene_color` shader-readable and
            // the prior presentation left this frame-slot output readable.
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &before,
            );
        }

        let color_layers = vk::ImageSubresourceLayers::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .layer_count(1);
        let region = vk::ImageBlit::default()
            .src_subresource(color_layers)
            .src_offsets([
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: self.extents.render.width as i32,
                    y: self.extents.render.height as i32,
                    z: 1,
                },
            ])
            .dst_subresource(color_layers)
            .dst_offsets([
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: self.extents.output.width as i32,
                    y: self.extents.output.height as i32,
                    z: 1,
                },
            ]);
        unsafe {
            // SAFETY: both images are single-sample RGBA16F color images in
            // the transfer layouts established above; the regions match their
            // declared extents and linear blit is supported for this format on
            // the renderer's desktop Vulkan target.
            device.cmd_blit_image(
                cmd,
                scene_color,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                output,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
                vk::Filter::LINEAR,
            );
        }

        let after = [
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image(scene_color)
                .subresource_range(range),
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image(output)
                .subresource_range(range),
        ];
        unsafe {
            // SAFETY: orders the completed blit before presentation sampling
            // and restores both descriptors' declared shader-read layouts.
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &after,
            );
        }
    }

    unsafe fn record_fsr_barriers_before(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        inputs: UpscaleDispatchInputs,
    ) {
        let color = color_subresource_single_mip();
        let depth = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::DEPTH)
            .level_count(1)
            .layer_count(1);
        let barriers = [
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image(inputs.scene_color)
                .subresource_range(color),
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image(inputs.motion_vectors)
                .subresource_range(color),
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_READ)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image(inputs.depth)
                .subresource_range(depth),
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_READ)
                .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(self.output_images[frame])
                .subresource_range(color),
        ];
        unsafe {
            // SAFETY: all images are in the old layouts established by their
            // producer passes / prior presentation; `cmd` is recording.
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &barriers,
            );
        }
    }

    unsafe fn record_fsr_barriers_after(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        depth_image: vk::Image,
    ) {
        let barriers = [
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_READ)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .new_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)
                .image(depth_image)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::DEPTH)
                        .level_count(1)
                        .layer_count(1),
                ),
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image(self.output_images[frame])
                .subresource_range(color_subresource_single_mip()),
        ];
        unsafe {
            // SAFETY: the SDK dispatch above finished recording all compute
            // accesses before this barrier in the same primary command buffer.
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &barriers,
            );
        }
    }

    /// Returns true once, after queue submission, for a successfully recorded
    /// FSR dispatch. The caller uses this to consume jitter/reset history only
    /// when the corresponding GPU work was actually submitted.
    pub fn take_submitted_dispatch(&mut self) -> bool {
        std::mem::take(&mut self.dispatched_this_frame)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn recreate(
        &mut self,
        instance: &ash::Instance,
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        extents: FrameExtentSet,
    ) -> Result<()> {
        let mode = self.mode;
        // SAFETY: resize calls this only after `device_wait_idle`.
        unsafe { self.destroy(device, allocator) };
        *self = Self::new(
            instance,
            device,
            physical_device,
            allocator,
            queue,
            command_pool,
            mode,
            extents,
        )?;
        Ok(())
    }

    /// # Safety
    ///
    /// The device must be idle and every descriptor/command buffer referencing
    /// these output views must no longer be executing.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        self.context.take();
        for view in self.output_views.drain(..) {
            unsafe { device.destroy_image_view(view, None) };
        }
        for image in self.output_images.drain(..) {
            unsafe { device.destroy_image(image, None) };
        }
        for allocation in self.output_allocations.drain(..).flatten() {
            allocator
                .lock()
                .expect("allocator lock poisoned")
                .free(allocation)
                .ok();
        }
        self.dispatched_this_frame = false;
    }
}

fn vulkan_image(
    image: vk::Image,
    format: vk::Format,
    usage: vk::ImageUsageFlags,
    size: [u32; 2],
) -> fsr3::VulkanImage {
    fsr3::VulkanImage {
        image: image.as_raw(),
        format: format.as_raw() as u32,
        usage: usage.as_raw(),
        size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_contract_is_storage_sampled_and_native_copyable() {
        let usage = vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_DST;
        assert!(usage.contains(vk::ImageUsageFlags::STORAGE));
        assert!(usage.contains(vk::ImageUsageFlags::SAMPLED));
        assert!(usage.contains(vk::ImageUsageFlags::TRANSFER_DST));
        assert_eq!(HDR_FORMAT, vk::Format::R16G16B16A16_SFLOAT);
    }
}
