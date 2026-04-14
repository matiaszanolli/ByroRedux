//! Screenshot capture — copies the composited swapchain image to a staging
//! buffer for CPU readback and PNG encoding.

use super::VulkanContext;
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;
use gpu_allocator::MemoryLocation;
use std::sync::atomic::Ordering;

impl VulkanContext {
    /// If a previous frame requested a screenshot and the GPU has completed,
    /// read back the staging buffer, encode as PNG, and store the result.
    ///
    /// Called at the top of `draw_frame()` after the fence wait (GPU done).
    pub(super) fn screenshot_finish_readback(&mut self) {
        if !self.screenshot_pending_readback {
            return;
        }
        self.screenshot_pending_readback = false;

        let Some((_, ref allocation, size)) = self.screenshot_staging else {
            return;
        };

        let width = self.swapchain_state.extent.width;
        let height = self.swapchain_state.extent.height;

        // Read the staging buffer.
        let data = match allocation.mapped_slice() {
            Some(slice) => &slice[..size as usize],
            None => {
                log::warn!("Screenshot staging buffer not mapped");
                return;
            }
        };

        // Swapchain format is B8G8R8A8_SRGB — convert BGRA → RGBA for PNG.
        let mut rgba = Vec::with_capacity(data.len());
        for pixel in data.chunks_exact(4) {
            rgba.push(pixel[2]); // R
            rgba.push(pixel[1]); // G
            rgba.push(pixel[0]); // B
            rgba.push(pixel[3]); // A
        }

        // Encode as PNG.
        let mut png_bytes = Vec::new();
        {
            let encoder = image::codecs::png::PngEncoder::new(std::io::Cursor::new(&mut png_bytes));
            use image::ImageEncoder;
            if let Err(e) = encoder.write_image(
                &rgba,
                width,
                height,
                image::ColorType::Rgba8,
            ) {
                log::warn!("Screenshot PNG encode failed: {e}");
                return;
            }
        }

        log::info!(
            "Screenshot captured: {}x{}, {} bytes PNG",
            width,
            height,
            png_bytes.len()
        );

        // Store result for the debug server to pick up.
        *self.screenshot_result.lock().unwrap() = Some(png_bytes);
    }

    /// If a screenshot was requested, record copy commands from the swapchain
    /// image to a staging buffer.
    ///
    /// Called in `draw_frame()` after composite dispatch, before `end_command_buffer`.
    /// The swapchain image is in `PRESENT_SRC_KHR` layout after the composite pass.
    pub(super) unsafe fn screenshot_record_copy(
        &mut self,
        cmd: vk::CommandBuffer,
        swapchain_image: vk::Image,
    ) {
        if !self.screenshot_requested.swap(false, Ordering::AcqRel) {
            return;
        }

        let width = self.swapchain_state.extent.width;
        let height = self.swapchain_state.extent.height;
        let pixel_size: vk::DeviceSize = 4; // B8G8R8A8
        let buffer_size = width as vk::DeviceSize * height as vk::DeviceSize * pixel_size;

        // Ensure staging buffer exists and is large enough.
        self.ensure_screenshot_staging(buffer_size);

        let Some((staging_buffer, _, _)) = self.screenshot_staging.as_ref() else {
            log::warn!("Screenshot staging buffer creation failed");
            return;
        };
        let staging_buffer = *staging_buffer;

        // Transition swapchain image: PRESENT_SRC_KHR → TRANSFER_SRC_OPTIMAL
        let barrier_to_transfer = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .image(swapchain_image)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .level_count(1)
                    .layer_count(1),
            );

        self.device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier_to_transfer],
        );

        // Copy image → buffer.
        let region = vk::BufferImageCopy::default()
            .buffer_offset(0)
            .buffer_row_length(0) // tightly packed
            .buffer_image_height(0)
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .mip_level(0)
                    .base_array_layer(0)
                    .layer_count(1),
            )
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            });

        self.device.cmd_copy_image_to_buffer(
            cmd,
            swapchain_image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            staging_buffer,
            &[region],
        );

        // Transition back: TRANSFER_SRC_OPTIMAL → PRESENT_SRC_KHR
        let barrier_to_present = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .src_access_mask(vk::AccessFlags::TRANSFER_READ)
            .dst_access_mask(vk::AccessFlags::empty())
            .image(swapchain_image)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .level_count(1)
                    .layer_count(1),
            );

        self.device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier_to_present],
        );

        self.screenshot_pending_readback = true;
    }

    /// Ensure a host-visible staging buffer exists for screenshot readback.
    fn ensure_screenshot_staging(&mut self, required_size: vk::DeviceSize) {
        // Already large enough?
        if let Some((_, _, existing_size)) = &self.screenshot_staging {
            if *existing_size >= required_size {
                return;
            }
            // Too small — destroy and recreate.
            self.destroy_screenshot_staging();
        }

        let Some(ref alloc) = self.allocator else {
            return;
        };

        let buffer_info = vk::BufferCreateInfo::default()
            .size(required_size)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe {
            match self.device.create_buffer(&buffer_info, None) {
                Ok(b) => b,
                Err(e) => {
                    log::warn!("Screenshot staging buffer creation failed: {e}");
                    return;
                }
            }
        };

        let requirements = unsafe { self.device.get_buffer_memory_requirements(buffer) };

        let allocation = {
            let mut allocator = alloc.lock().unwrap();
            match allocator.allocate(&vk_alloc::AllocationCreateDesc {
                name: "screenshot-staging",
                requirements,
                location: MemoryLocation::GpuToCpu,
                linear: true,
                allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
            }) {
                Ok(a) => a,
                Err(e) => {
                    log::warn!("Screenshot staging allocation failed: {e}");
                    unsafe { self.device.destroy_buffer(buffer, None) };
                    return;
                }
            }
        };

        unsafe {
            if let Err(e) =
                self.device
                    .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
            {
                log::warn!("Screenshot staging bind failed: {e}");
                let mut allocator = alloc.lock().unwrap();
                let _ = allocator.free(allocation);
                self.device.destroy_buffer(buffer, None);
                return;
            }
        }

        self.screenshot_staging = Some((buffer, allocation, required_size));
    }

    pub(super) fn destroy_screenshot_staging(&mut self) {
        if let Some((buffer, allocation, _)) = self.screenshot_staging.take() {
            unsafe { self.device.destroy_buffer(buffer, None) };
            if let Some(ref alloc) = self.allocator {
                let mut allocator = alloc.lock().unwrap();
                let _ = allocator.free(allocation);
            }
        }
    }
}
