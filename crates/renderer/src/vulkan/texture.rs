//! GPU texture: image upload via staging buffer, layout transitions, sampler.

use super::allocator::SharedAllocator;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;
use gpu_allocator::MemoryLocation;

/// A GPU-resident texture with image, view, and sampler.
pub struct Texture {
    pub image: vk::Image,
    pub image_view: vk::ImageView,
    pub sampler: vk::Sampler,
    allocation: Option<vk_alloc::Allocation>,
}

impl Texture {
    /// Create a texture from raw RGBA pixel data.
    ///
    /// Full upload pipeline:
    /// 1. Create staging buffer (CPU-visible), copy pixel data
    /// 2. Create device-local image (TRANSFER_DST | SAMPLED, R8G8B8A8_SRGB)
    /// 3. Transition layout: UNDEFINED → TRANSFER_DST_OPTIMAL
    /// 4. Copy staging buffer → image
    /// 5. Transition layout: TRANSFER_DST_OPTIMAL → SHADER_READ_ONLY_OPTIMAL
    /// 6. Destroy staging buffer
    /// 7. Create image view + sampler
    pub fn from_rgba(
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<Self> {
        assert_eq!(
            pixels.len(),
            (width * height * 4) as usize,
            "pixel data must be width*height*4 RGBA bytes"
        );

        let image_size = pixels.len() as vk::DeviceSize;

        // 1. Staging buffer.
        let staging_buffer_info = vk::BufferCreateInfo::default()
            .size(image_size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let staging_buffer = unsafe {
            device
                .create_buffer(&staging_buffer_info, None)
                .context("Failed to create staging buffer")?
        };

        let staging_reqs = unsafe { device.get_buffer_memory_requirements(staging_buffer) };

        let mut staging_alloc = allocator
            .lock()
            .expect("allocator lock poisoned")
            .allocate(&vk_alloc::AllocationCreateDesc {
                name: "texture_staging",
                requirements: staging_reqs,
                location: MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate staging memory")?;

        unsafe {
            device
                .bind_buffer_memory(
                    staging_buffer,
                    staging_alloc.memory(),
                    staging_alloc.offset(),
                )
                .context("Failed to bind staging buffer")?;
        }

        // Copy pixels into staging.
        staging_alloc
            .mapped_slice_mut()
            .context("Staging buffer not mapped")?[..pixels.len()]
            .copy_from_slice(pixels);

        // 2. Device-local image.
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_SRGB)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = unsafe {
            device
                .create_image(&image_info, None)
                .context("Failed to create texture image")?
        };

        let image_reqs = unsafe { device.get_image_memory_requirements(image) };

        let image_alloc = allocator
            .lock()
            .expect("allocator lock poisoned")
            .allocate(&vk_alloc::AllocationCreateDesc {
                name: "texture_image",
                requirements: image_reqs,
                location: MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate texture image memory")?;

        unsafe {
            device
                .bind_image_memory(image, image_alloc.memory(), image_alloc.offset())
                .context("Failed to bind texture image memory")?;
        }

        // 3-5. Layout transitions + copy via one-time commands.
        with_one_time_commands(device, queue, command_pool, |cmd| {
            // 3. UNDEFINED → TRANSFER_DST_OPTIMAL
            let barrier_to_dst = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);

            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier_to_dst],
                );
            }

            // 4. Copy buffer → image.
            let region = vk::BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                image_extent: vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                },
            };

            unsafe {
                device.cmd_copy_buffer_to_image(
                    cmd,
                    staging_buffer,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[region],
                );
            }

            // 5. TRANSFER_DST_OPTIMAL → SHADER_READ_ONLY_OPTIMAL
            let barrier_to_read = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ);

            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier_to_read],
                );
            }
        })?;

        // 6. Destroy staging buffer.
        unsafe {
            device.destroy_buffer(staging_buffer, None);
        }
        allocator
            .lock()
            .expect("allocator lock poisoned")
            .free(staging_alloc)
            .context("Failed to free staging allocation")?;

        // 7. Image view.
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_SRGB)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let image_view = unsafe {
            device
                .create_image_view(&view_info, None)
                .context("Failed to create texture image view")?
        };

        // 8. Sampler.
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .anisotropy_enable(false)
            .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
            .unnormalized_coordinates(false)
            .compare_enable(false)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR);

        let sampler = unsafe {
            device
                .create_sampler(&sampler_info, None)
                .context("Failed to create texture sampler")?
        };

        log::debug!("Texture uploaded: {}x{} RGBA", width, height);

        Ok(Self {
            image,
            image_view,
            sampler,
            allocation: Some(image_alloc),
        })
    }

    /// Create a texture from block-compressed (BC) data — DDS payload with mip chain.
    ///
    /// Uploads compressed data directly to the GPU; no CPU decompression.
    /// BC1/BC2/BC3 are mandatory in Vulkan core.
    pub fn from_bc(
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        meta: &super::dds::DdsMetadata,
        pixel_data: &[u8],
    ) -> Result<Self> {
        use super::dds;

        let total_size = dds::total_data_size(meta);
        assert!(
            pixel_data.len() as u64 >= total_size,
            "BC pixel data too small: {} bytes for {}x{} {} mips",
            pixel_data.len(),
            meta.width,
            meta.height,
            meta.mip_count,
        );

        let image_size = total_size as vk::DeviceSize;

        // 1. Staging buffer.
        let staging_buffer_info = vk::BufferCreateInfo::default()
            .size(image_size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let staging_buffer = unsafe {
            device
                .create_buffer(&staging_buffer_info, None)
                .context("Failed to create BC staging buffer")?
        };

        let staging_reqs = unsafe { device.get_buffer_memory_requirements(staging_buffer) };

        let mut staging_alloc = allocator
            .lock()
            .expect("allocator lock poisoned")
            .allocate(&vk_alloc::AllocationCreateDesc {
                name: "bc_texture_staging",
                requirements: staging_reqs,
                location: MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate BC staging memory")?;

        unsafe {
            device
                .bind_buffer_memory(
                    staging_buffer,
                    staging_alloc.memory(),
                    staging_alloc.offset(),
                )
                .context("Failed to bind BC staging buffer")?;
        }

        staging_alloc
            .mapped_slice_mut()
            .context("BC staging buffer not mapped")?[..total_size as usize]
            .copy_from_slice(&pixel_data[..total_size as usize]);

        // 2. Device-local image.
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(meta.format)
            .extent(vk::Extent3D {
                width: meta.width,
                height: meta.height,
                depth: 1,
            })
            .mip_levels(meta.mip_count)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = unsafe {
            device
                .create_image(&image_info, None)
                .context("Failed to create BC texture image")?
        };

        let image_reqs = unsafe { device.get_image_memory_requirements(image) };

        let image_alloc = allocator
            .lock()
            .expect("allocator lock poisoned")
            .allocate(&vk_alloc::AllocationCreateDesc {
                name: "bc_texture_image",
                requirements: image_reqs,
                location: MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate BC texture image memory")?;

        unsafe {
            device
                .bind_image_memory(image, image_alloc.memory(), image_alloc.offset())
                .context("Failed to bind BC texture image memory")?;
        }

        // Build per-mip copy regions.
        let mut regions = Vec::with_capacity(meta.mip_count as usize);
        let mut buffer_offset: vk::DeviceSize = 0;
        for mip in 0..meta.mip_count {
            let mip_w = (meta.width >> mip).max(1);
            let mip_h = (meta.height >> mip).max(1);
            let mip_bytes = dds::mip_size(
                meta.width,
                meta.height,
                mip,
                meta.block_size,
                meta.compressed,
            );

            regions.push(vk::BufferImageCopy {
                buffer_offset,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: mip,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                image_extent: vk::Extent3D {
                    width: mip_w,
                    height: mip_h,
                    depth: 1,
                },
            });
            buffer_offset += mip_bytes as vk::DeviceSize;
        }

        // 3-5. Layout transitions + copy.
        with_one_time_commands(device, queue, command_pool, |cmd| {
            let barrier_to_dst = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: meta.mip_count,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);

            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier_to_dst],
                );

                device.cmd_copy_buffer_to_image(
                    cmd,
                    staging_buffer,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &regions,
                );
            }

            let barrier_to_read = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: meta.mip_count,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ);

            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier_to_read],
                );
            }
        })?;

        // 6. Destroy staging.
        unsafe {
            device.destroy_buffer(staging_buffer, None);
        }
        allocator
            .lock()
            .expect("allocator lock poisoned")
            .free(staging_alloc)
            .context("Failed to free BC staging allocation")?;

        // 7. Image view.
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(meta.format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: meta.mip_count,
                base_array_layer: 0,
                layer_count: 1,
            });

        let image_view = unsafe {
            device
                .create_image_view(&view_info, None)
                .context("Failed to create BC texture image view")?
        };

        // 8. Sampler with mip support.
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .anisotropy_enable(false)
            .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
            .unnormalized_coordinates(false)
            .compare_enable(false)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .min_lod(0.0)
            .max_lod(meta.mip_count as f32);

        let sampler = unsafe {
            device
                .create_sampler(&sampler_info, None)
                .context("Failed to create BC texture sampler")?
        };

        log::info!(
            "BC texture uploaded: {}x{}, {:?}, {} mips",
            meta.width,
            meta.height,
            meta.format,
            meta.mip_count,
        );

        Ok(Self {
            image,
            image_view,
            sampler,
            allocation: Some(image_alloc),
        })
    }

    /// Create a texture from DDS file bytes (header + pixel data).
    ///
    /// Parses the DDS header, then uploads via `from_bc` or `from_rgba` as appropriate.
    pub fn from_dds(
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        dds_bytes: &[u8],
    ) -> Result<Self> {
        let meta = super::dds::parse_dds(dds_bytes)?;
        let pixel_data = &dds_bytes[meta.data_offset..];

        if meta.compressed {
            Self::from_bc(device, allocator, queue, command_pool, &meta, pixel_data)
        } else {
            // Uncompressed RGBA — use existing from_rgba path
            Self::from_rgba(
                device,
                allocator,
                queue,
                command_pool,
                meta.width,
                meta.height,
                pixel_data,
            )
        }
    }

    /// Destroy the texture and free GPU memory.
    pub fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        unsafe {
            device.destroy_sampler(self.sampler, None);
            device.destroy_image_view(self.image_view, None);
        }
        if let Some(alloc) = self.allocation.take() {
            allocator
                .lock()
                .expect("allocator lock poisoned")
                .free(alloc)
                .expect("Failed to free texture allocation");
        }
        unsafe {
            device.destroy_image(self.image, None);
        }
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        if self.allocation.is_some() {
            log::warn!(
                "Texture dropped without destroy() — VkImage, VkImageView, VkSampler, and GPU allocation leaked"
            );
            debug_assert!(false, "Texture leaked: call destroy() before dropping");
        }
    }
}

/// Execute a one-time-submit command buffer: allocate, record, submit, wait, free.
///
/// The queue `Mutex` is locked only for the submit+wait, not during recording.
pub(crate) fn with_one_time_commands<F: FnOnce(vk::CommandBuffer)>(
    device: &ash::Device,
    queue: &std::sync::Mutex<vk::Queue>,
    pool: vk::CommandPool,
    f: F,
) -> Result<()> {
    let alloc_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);

    let cmd = unsafe {
        device
            .allocate_command_buffers(&alloc_info)
            .context("Failed to allocate one-time command buffer")?[0]
    };

    let begin_info =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

    unsafe {
        device
            .begin_command_buffer(cmd, &begin_info)
            .context("begin one-time command buffer")?;
    }

    f(cmd);

    unsafe {
        device
            .end_command_buffer(cmd)
            .context("end one-time command buffer")?;
    }

    let submit_info = vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&cmd));

    unsafe {
        let q = *queue.lock().expect("graphics queue lock poisoned");
        device
            .queue_submit(q, &[submit_info], vk::Fence::null())
            .context("submit one-time commands")?;
        device
            .queue_wait_idle(q)
            .context("wait for one-time commands")?;
        device.free_command_buffers(pool, &[cmd]);
    }

    Ok(())
}

/// Generate a checkerboard RGBA pixel buffer (no file I/O needed).
pub fn generate_checkerboard(width: u32, height: u32, cell_size: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let checker = ((x / cell_size) + (y / cell_size)) % 2 == 0;
            let (r, g, b) = if checker {
                (220u8, 220, 220)
            } else {
                (80, 80, 80)
            };
            pixels.extend_from_slice(&[r, g, b, 255]);
        }
    }
    pixels
}
