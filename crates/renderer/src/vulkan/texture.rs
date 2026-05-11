//! GPU texture: image upload via staging buffer, layout transitions, sampler.

use super::allocator::SharedAllocator;
use super::buffer::{StagingGuard, StagingPool};
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
    /// Stashed at construction so `Drop` can self-free if `destroy()`
    /// was missed — the canonical lifecycle is still
    /// `TextureRegistry::tick_deferred_destroy` calling `destroy()`
    /// after the safe MAX_FRAMES_IN_FLIGHT delay, but textures that
    /// escape the registry (panic mid-cell-load, direct-construction
    /// callers, future code paths) now release their VkImage,
    /// VkImageView, and gpu_allocator slab via Drop instead of
    /// silently leaking. Cloning is cheap: `ash::Device` is a thin
    /// `Arc`-backed handle and `SharedAllocator` is already
    /// `Arc<Mutex<…>>`. Sampler is shared and owned elsewhere
    /// (`TextureRegistry`) so neither path touches it. #656.
    device: ash::Device,
    /// `Option` so `destroy()` can release the Arc clone immediately
    /// after freeing the underlying allocation. Same shutdown-leak
    /// fix as `GpuBuffer` (#927). Once `allocation` is `None`, the
    /// allocator is no longer needed (`Drop` short-circuits the
    /// self-clean), so dropping the Arc here is safe.
    allocator: Option<SharedAllocator>,
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
    /// 7. Create image view (sampler provided externally)
    pub fn from_rgba(
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        width: u32,
        height: u32,
        pixels: &[u8],
        sampler: vk::Sampler,
        mut staging_pool: Option<&mut StagingPool>,
    ) -> Result<Self> {
        assert_eq!(
            pixels.len(),
            (width * height * 4) as usize,
            "pixel data must be width*height*4 RGBA bytes"
        );

        let image_size = pixels.len() as vk::DeviceSize;

        // 1. Staging buffer — from pool (reuse) or fresh allocate. See
        //    #239 — pre-fix texture uploads bypassed the pool entirely.
        let (staging_buffer, mut staging_alloc) = if let Some(pool) = staging_pool.as_deref_mut() {
            pool.acquire(image_size)?
        } else {
            let staging_buffer_info = vk::BufferCreateInfo::default()
                .size(image_size)
                .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            let buf = unsafe {
                device
                    .create_buffer(&staging_buffer_info, None)
                    .context("Failed to create staging buffer")?
            };
            let reqs = unsafe { device.get_buffer_memory_requirements(buf) };
            let alloc = allocator
                .lock()
                .expect("allocator lock poisoned")
                .allocate(&vk_alloc::AllocationCreateDesc {
                    name: "texture_staging",
                    requirements: reqs,
                    location: MemoryLocation::CpuToGpu,
                    linear: true,
                    allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
                })
                .context("Failed to allocate staging memory")?;
            super::buffer::debug_assert_cpu_to_gpu_mapped(&alloc, "texture_staging");
            unsafe {
                device
                    .bind_buffer_memory(buf, alloc.memory(), alloc.offset())
                    .context("Failed to bind staging buffer")?;
            }
            (buf, alloc)
        };

        // Copy pixels into staging.
        staging_alloc
            .mapped_slice_mut()
            .context("Staging buffer not mapped")?[..pixels.len()]
            .copy_from_slice(pixels);

        // Wrap staging in RAII guard — ensures cleanup on early return.
        let staging = StagingGuard::new(
            staging_buffer,
            staging_alloc,
            device.clone(),
            allocator.clone(),
        );

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
                    staging.buffer,
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
            Ok(())
        })?;

        // 6. Release staging buffer. When a pool was provided, hand
        //    the buffer back for reuse; otherwise destroy outright.
        //    Guard ensures cleanup on error above regardless.
        if let Some(pool) = staging_pool {
            let capacity = staging
                .allocation
                .as_ref()
                .map(|a| a.size())
                .unwrap_or(image_size);
            staging.release_to(pool, capacity);
        } else {
            staging.destroy();
        }

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

        log::debug!("Texture uploaded: {}x{} RGBA", width, height);

        Ok(Self {
            image,
            image_view,
            sampler,
            allocation: Some(image_alloc),
            device: device.clone(),
            allocator: Some(allocator.clone()),
        })
    }

    /// Create a texture from a DDS pixel-data payload with its full
    /// authored mip chain.
    ///
    /// Handles both block-compressed (BC1/BC2/BC3/BC4/BC5/BC7) and
    /// uncompressed RGBA formats — `meta.compressed` flips the per-mip
    /// byte-size math in `dds::mip_size` and the rest of the upload
    /// (image creation with `meta.mip_count`, per-mip
    /// `BufferImageCopy` regions, image view `level_count`) is
    /// format-agnostic. Pre-#730 closeout the uncompressed path went
    /// through `from_rgba` which hard-coded `mip_levels(1)` and
    /// dropped the authored mip chain — uncompressed cloud sprites
    /// then aliased visibly under minification because the sampler
    /// could only ever read mip 0.
    pub fn from_dds_with_mip_chain(
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        meta: &super::dds::DdsMetadata,
        pixel_data: &[u8],
        sampler: vk::Sampler,
        mut staging_pool: Option<&mut StagingPool>,
    ) -> Result<Self> {
        // Self-contained wrapper around [`Self::record_dds_upload`].
        // Records the upload into a one-time command buffer, submits +
        // fence-waits ONCE, then releases the staging buffer. Used by
        // call sites that want the legacy synchronous semantics
        // (single-NIF render, debug paths).
        //
        // Cell-load and other bulk paths should instead route through
        // `TextureRegistry::enqueue_dds_with_clamp` +
        // `flush_pending_uploads` so dozens of textures share ONE
        // submit + fence-wait. See #881.
        let mut texture_holder: Option<Self> = None;
        let mut staging_holder: Option<StagingGuard> = None;
        let mut image_size_holder: vk::DeviceSize = 0;

        with_one_time_commands(device, queue, command_pool, |cmd| {
            let (texture, staging, image_size) = Self::record_dds_upload(
                device,
                allocator,
                cmd,
                meta,
                pixel_data,
                sampler,
                staging_pool.as_deref_mut(),
            )?;
            texture_holder = Some(texture);
            staging_holder = Some(staging);
            image_size_holder = image_size;
            Ok(())
        })?;

        let texture = texture_holder.expect("record_dds_upload populated texture");
        let staging = staging_holder.expect("record_dds_upload populated staging");

        // Release staging — back to pool (reuse) or destroy. Safe to
        // do here because the fence wait inside `with_one_time_commands`
        // has already returned, so the GPU is done reading the staging
        // buffer.
        if let Some(pool) = staging_pool {
            let capacity = staging
                .allocation
                .as_ref()
                .map(|a| a.size())
                .unwrap_or(image_size_holder);
            staging.release_to(pool, capacity);
        } else {
            staging.destroy();
        }

        Ok(texture)
    }

    /// Record-only stage of a DDS upload — allocates the GPU image,
    /// allocates a staging buffer, copies CPU pixel data into staging,
    /// and RECORDS the layout-transition + copy pair into the provided
    /// command buffer. Returns the partially-built `Texture`
    /// (image + view + sampler), the `StagingGuard` the caller MUST
    /// retain until after the submit + fence-wait completes, and the
    /// staging buffer's effective size (for `StagingGuard::release_to`).
    ///
    /// Stage B (submit + wait) and Stage C (release staging) are the
    /// caller's responsibility. Use this entry point when batching
    /// many DDS uploads into ONE submit (see
    /// `TextureRegistry::flush_pending_uploads`); for a single-shot
    /// upload, use [`Self::from_dds_with_mip_chain`] instead which
    /// bundles all three stages.
    ///
    /// SAFETY: the command buffer must be in the recording state. The
    /// returned StagingGuard's underlying VkBuffer is referenced by
    /// the recorded `cmd_copy_buffer_to_image`, so dropping it before
    /// the GPU has finished executing the cmd would produce a
    /// use-after-free. See #881 / CELL-PERF-03.
    pub(crate) fn record_dds_upload(
        device: &ash::Device,
        allocator: &SharedAllocator,
        cmd: vk::CommandBuffer,
        meta: &super::dds::DdsMetadata,
        pixel_data: &[u8],
        sampler: vk::Sampler,
        mut staging_pool: Option<&mut StagingPool>,
    ) -> Result<(Self, StagingGuard, vk::DeviceSize)> {
        use super::dds;

        let total_size = dds::total_data_size(meta);
        assert!(
            pixel_data.len() as u64 >= total_size,
            "DDS pixel data too small: {} bytes for {}x{} {:?} {} mips ({} expected)",
            pixel_data.len(),
            meta.width,
            meta.height,
            meta.format,
            meta.mip_count,
            total_size,
        );

        let image_size = total_size as vk::DeviceSize;

        // 1. Staging buffer — from pool (reuse) or fresh. See #239.
        let (staging_buffer, mut staging_alloc) = if let Some(pool) = staging_pool.as_deref_mut() {
            pool.acquire(image_size)?
        } else {
            let staging_buffer_info = vk::BufferCreateInfo::default()
                .size(image_size)
                .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            let buf = unsafe {
                device
                    .create_buffer(&staging_buffer_info, None)
                    .context("Failed to create DDS staging buffer")?
            };
            let reqs = unsafe { device.get_buffer_memory_requirements(buf) };
            let alloc = allocator
                .lock()
                .expect("allocator lock poisoned")
                .allocate(&vk_alloc::AllocationCreateDesc {
                    name: "dds_texture_staging",
                    requirements: reqs,
                    location: MemoryLocation::CpuToGpu,
                    linear: true,
                    allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
                })
                .context("Failed to allocate DDS staging memory")?;
            super::buffer::debug_assert_cpu_to_gpu_mapped(&alloc, "dds_texture_staging");
            unsafe {
                device
                    .bind_buffer_memory(buf, alloc.memory(), alloc.offset())
                    .context("Failed to bind DDS staging buffer")?;
            }
            (buf, alloc)
        };

        staging_alloc
            .mapped_slice_mut()
            .context("DDS staging buffer not mapped")?[..total_size as usize]
            .copy_from_slice(&pixel_data[..total_size as usize]);

        // Wrap staging in RAII guard — ensures cleanup on early return.
        let staging = StagingGuard::new(
            staging_buffer,
            staging_alloc,
            device.clone(),
            allocator.clone(),
        );

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
                .context("Failed to create DDS texture image")?
        };

        let image_reqs = unsafe { device.get_image_memory_requirements(image) };

        let image_alloc = allocator
            .lock()
            .expect("allocator lock poisoned")
            .allocate(&vk_alloc::AllocationCreateDesc {
                name: "dds_texture_image",
                requirements: image_reqs,
                location: MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate DDS texture image memory")?;

        unsafe {
            device
                .bind_image_memory(image, image_alloc.memory(), image_alloc.offset())
                .context("Failed to bind DDS texture image memory")?;
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

        // 3-5. Record layout transitions + copy into the provided cmd.
        // Per-image barriers (not global) so multiple uploads recorded
        // into the same cmd don't serialise on each other unnecessarily.
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
                staging.buffer,
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

        // 7. Image view (CPU-only, independent of GPU work).
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
                .context("Failed to create DDS texture image view")?
        };

        log::info!(
            "DDS texture recorded: {}x{}, {:?}, {} mips",
            meta.width,
            meta.height,
            meta.format,
            meta.mip_count,
        );

        Ok((
            Self {
                image,
                image_view,
                sampler,
                allocation: Some(image_alloc),
                device: device.clone(),
                allocator: Some(allocator.clone()),
            },
            staging,
            image_size,
        ))
    }

    /// Create a texture from DDS file bytes (header + pixel data).
    ///
    /// Parses the DDS header, then uploads via
    /// [`Self::from_dds_with_mip_chain`] regardless of whether the
    /// payload is block-compressed or uncompressed RGBA — both paths
    /// share the same per-mip upload shape, only the byte-size math
    /// changes (and `dds::mip_size` already gates that on
    /// `meta.compressed`). Pre-#730 the uncompressed branch routed
    /// through `from_rgba` which hard-coded `mip_levels(1)` and lost
    /// every authored mip below 0 — uncompressed cloud sprites were
    /// the visible victim.
    pub fn from_dds(
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        dds_bytes: &[u8],
        sampler: vk::Sampler,
        staging_pool: Option<&mut StagingPool>,
    ) -> Result<Self> {
        let meta = super::dds::parse_dds(dds_bytes)?;
        let pixel_data = &dds_bytes[meta.data_offset..];

        Self::from_dds_with_mip_chain(
            device,
            allocator,
            queue,
            command_pool,
            &meta,
            pixel_data,
            sampler,
            staging_pool,
        )
    }

    /// Destroy the texture and free GPU memory.
    ///
    /// Does NOT destroy the sampler — it's shared across all textures
    /// and owned by TextureRegistry.
    pub fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        // SAFETY: Vulkan object destruction order: view → image → allocation.
        // The image view references the image; the image binds the allocation.
        // Freeing the allocation before destroying the image is a
        // use-after-free on the GPU memory backing. See issue #18.
        unsafe {
            device.destroy_image_view(self.image_view, None);
            device.destroy_image(self.image, None);
        }
        if let Some(alloc) = self.allocation.take() {
            allocator
                .lock()
                .expect("allocator lock poisoned")
                .free(alloc)
                .expect("Failed to free texture allocation");
        }
        // #927 — release the stored allocator Arc clone now that the
        // GPU side is freed. Without this, every Texture struct kept
        // a live Arc until naturally dropped (post-`VulkanContext::Drop`),
        // contributing to the outstanding-refs leak path. Drop's
        // safety-net branch (only hit when destroy() was skipped)
        // handles the None case.
        self.allocator = None;
    }
}

impl Drop for Texture {
    /// Safety net: when `TextureRegistry::tick_deferred_destroy`
    /// already called `destroy()`, `allocation` is `None` and this
    /// path is a no-op. When the registry path is bypassed (panic
    /// mid-cell-load, direct ad-hoc Texture, etc.) Drop self-cleans
    /// using the stashed device + allocator handles instead of
    /// silently leaking VkImage / VkImageView and the gpu_allocator
    /// slab. Sampler is shared (owned by `TextureRegistry`) so neither
    /// path touches it. Pre-#656 release builds dropped the
    /// allocation handle on the floor — `gpu_allocator::Allocation::Drop`
    /// does not free, the slab kept the bytes, and every escaped
    /// Texture leaked four resources. The debug assertion is
    /// preserved as a louder signal in dev builds: hitting Drop with
    /// `allocation = Some` still indicates a missed destroy() in the
    /// canonical path and is worth investigating, even though Drop
    /// now releases the resources cleanly.
    fn drop(&mut self) {
        if self.allocation.is_none() {
            return;
        }
        log::warn!(
            "Texture dropped without destroy() — running cleanup from Drop (#656 safety net)",
        );
        debug_assert!(false, "Texture leaked into Drop: call destroy() first");
        unsafe {
            self.device.destroy_image_view(self.image_view, None);
            self.device.destroy_image(self.image, None);
        }
        if let Some(alloc) = self.allocation.take() {
            // Invariant: if `allocation` was `Some`, `allocator` is
            // also `Some` — `destroy()` clears them together (#927).
            // Hitting None here would mean the texture escaped
            // destroy() AND had its allocator cleared independently,
            // which is not a path the rest of the code takes.
            let Some(allocator) = self.allocator.as_ref() else {
                log::error!(
                    "Texture::Drop has live allocation but no allocator — \
                     slab leaks (was destroy() partially invoked?)",
                );
                return;
            };
            // Drop must not panic. Surface allocator failures as
            // log::error! and leak quietly rather than blowing up the
            // process from a destructor (e.g. on a poisoned mutex
            // during a panic unwind).
            match allocator.lock() {
                Ok(mut a) => {
                    if let Err(e) = a.free(alloc) {
                        log::error!("Texture::Drop failed to free allocation: {e}");
                    }
                }
                Err(_) => {
                    log::error!(
                        "Texture::Drop saw a poisoned allocator mutex — slab leaks deliberately to avoid double-panic",
                    );
                }
            }
        }
    }
}

/// Execute a one-time-submit command buffer: allocate, record, submit, wait, free.
///
/// The queue `Mutex` is locked only for the submit+wait, not during recording.
/// Run a closure in a one-time-submit command buffer, then wait for completion.
///
/// The closure returns `Result<()>` so recording errors (e.g. failed buffer
/// allocation mid-build) propagate out *without* submitting a partially-
/// recorded command buffer to the GPU. On closure failure the command buffer
/// is ended (Vulkan requires that before free) and freed without submission.
pub(crate) fn with_one_time_commands<F>(
    device: &ash::Device,
    queue: &std::sync::Mutex<vk::Queue>,
    pool: vk::CommandPool,
    f: F,
) -> Result<()>
where
    F: FnOnce(vk::CommandBuffer) -> Result<()>,
{
    with_one_time_commands_inner(device, queue, pool, None, f)
}

/// Variant of [`with_one_time_commands`] that reuses a persistent fence
/// instead of creating and destroying a new fence per submission.
///
/// Saves ~5us per call × ~700 calls during cell load (~3.5 ms total).
/// The fence must be created with no initial signal; this function will
/// reset it before submitting (#302).
pub(crate) fn with_one_time_commands_reuse_fence<F>(
    device: &ash::Device,
    queue: &std::sync::Mutex<vk::Queue>,
    pool: vk::CommandPool,
    fence: &std::sync::Mutex<vk::Fence>,
    f: F,
) -> Result<()>
where
    F: FnOnce(vk::CommandBuffer) -> Result<()>,
{
    with_one_time_commands_inner(device, queue, pool, Some(fence), f)
}

fn with_one_time_commands_inner<F>(
    device: &ash::Device,
    queue: &std::sync::Mutex<vk::Queue>,
    pool: vk::CommandPool,
    reusable_fence: Option<&std::sync::Mutex<vk::Fence>>,
    f: F,
) -> Result<()>
where
    F: FnOnce(vk::CommandBuffer) -> Result<()>,
{
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

    // Run the recording closure. If it fails, end + free the command buffer
    // (Vulkan spec requires end_command_buffer before free_command_buffers
    // when the buffer is in the recording state) and propagate the error
    // *without submitting*.
    if let Err(e) = f(cmd) {
        unsafe {
            // Best-effort end; ignore the result since we're already in an
            // error path. The buffer is then freed without submission.
            let _ = device.end_command_buffer(cmd);
            device.free_command_buffers(pool, &[cmd]);
        }
        return Err(e).context("one-time command recording failed; submission aborted");
    }

    unsafe {
        device
            .end_command_buffer(cmd)
            .context("end one-time command buffer")?;
    }

    let submit_info = vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&cmd));

    unsafe {
        // Use a dedicated fence instead of queue_wait_idle — waits only for
        // this submission, not the entire queue. Avoids serializing other
        // queue work during texture streaming or BLAS builds.
        //
        // If a reusable fence was provided (#302), lock + reset it. The
        // mutex serializes concurrent callers so only one submit+wait cycle
        // uses the fence at a time. Otherwise fall back to per-call
        // create/destroy for early-init paths that don't yet have a
        // persistent fence.
        let fence_guard = reusable_fence.map(|m| m.lock().expect("one-time fence lock poisoned"));
        let (fence, owned) = match fence_guard.as_ref() {
            Some(guard) => {
                device
                    .reset_fences(&[**guard])
                    .context("reset reusable one-time fence")?;
                (**guard, false)
            }
            None => {
                let f = device
                    .create_fence(&vk::FenceCreateInfo::default(), None)
                    .context("create one-time fence")?;
                (f, true)
            }
        };

        let q = *queue.lock().expect("graphics queue lock poisoned");
        device
            .queue_submit(q, &[submit_info], fence)
            .context("submit one-time commands")?;
        device
            .wait_for_fences(&[fence], true, u64::MAX)
            .context("wait for one-time commands")?;
        if owned {
            device.destroy_fence(fence, None);
        }
        drop(fence_guard);
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
