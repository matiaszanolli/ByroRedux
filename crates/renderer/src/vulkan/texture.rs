//! GPU texture: image upload via staging buffer, layout transitions, sampler.

use super::allocator::SharedAllocator;
use super::buffer::{StagingGuard, StagingPool};
use super::descriptors::{
    color_subresource_mips_layers, image_barrier_transfer_dst_to_shader_read_layers,
    image_barrier_undef_to_transfer_dst_layers,
};
use super::GpuUploadCtx;
use anyhow::{ensure, Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;
use gpu_allocator::MemoryLocation;

/// Image-view dimension used to select the matching bindless descriptor
/// binding (`sampler2D` vs `samplerCube`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureViewKind {
    D2,
    Cube,
}

impl TextureViewKind {
    pub const fn descriptor_binding(self) -> u32 {
        match self {
            Self::D2 => 0,
            Self::Cube => 1,
        }
    }
}

/// A GPU-resident texture with image, view, and sampler.
pub struct Texture {
    pub image: vk::Image,
    pub image_view: vk::ImageView,
    pub sampler: vk::Sampler,
    pub view_kind: TextureViewKind,
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
    /// Extent the image was created with. [`Self::can_update_rgba`] checks
    /// streaming uploads against it so a mismatched extent is refused (or,
    /// through `update_rgba`, recreates the image) instead of addressing a
    /// different-sized image (#4515 — previously nothing retained the
    /// creation extent).
    creation_extent: vk::Extent3D,
    /// Only single-mip, single-layer sRGB RGBA images accept raw replacements.
    rgba_updateable: bool,
}

impl Texture {
    pub(crate) fn can_update_rgba(&self, width: u32, height: u32) -> bool {
        self.rgba_updateable
            && self.creation_extent.width == width
            && self.creation_extent.height == height
    }

    /// Create a texture from raw RGBA pixel data.
    ///
    /// Thin wrapper around [`Self::from_dds_with_mip_chain`]: a 1-mip
    /// `R8G8B8A8_SRGB` image is just a degenerate DDS upload with
    /// `compressed=false` and `mip_count=1`. Pre-#1046 this path
    /// hand-coded its own staging-buffer-create + image-create +
    /// two-layout-transition-barriers dance — exactly what
    /// `record_dds_upload` already does. The split was the visible
    /// driver of #730 (uncompressed cloud sprites losing their
    /// authored mip chain because `from_rgba` hard-coded `mip_levels(1)`
    /// where `from_dds_with_mip_chain` would have respected the chain).
    pub fn from_rgba(
        ctx: GpuUploadCtx,
        width: u32,
        height: u32,
        pixels: &[u8],
        sampler: vk::Sampler,
        staging_pool: Option<&mut StagingPool>,
    ) -> Result<Self> {
        // from_rgba sizes the image at (width, height) itself, so the
        // creation-extent arm passes by construction; the payload arm is
        // the real check here.
        validate_rgba_upload(
            vk::Extent3D {
                width,
                height,
                depth: 1,
            },
            width,
            height,
            pixels.len(),
        )?;
        let meta = super::dds::DdsMetadata {
            width,
            height,
            mip_count: 1,
            format: vk::Format::R8G8B8A8_SRGB,
            block_size: 4, // bytes per pixel — uncompressed RGBA
            compressed: false,
            array_layers: 1,
            is_cubemap: false,
            data_offset: 0, // unused: caller passes the pixel slice directly
            expand: None,   // already R8G8B8A8
        };
        let texture = Self::from_dds_with_mip_chain(ctx, &meta, pixels, sampler, staging_pool)?;
        log::debug!("Texture uploaded: {}x{} RGBA", width, height);
        Ok(texture)
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
        ctx: GpuUploadCtx,
        meta: &super::dds::DdsMetadata,
        pixel_data: &[u8],
        sampler: vk::Sampler,
        mut staging_pool: Option<&mut StagingPool>,
    ) -> Result<Self> {
        let GpuUploadCtx {
            device,
            allocator,
            queue,
            command_pool,
        } = ctx;
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

        with_one_time_commands(device, queue, command_pool, |cmd| {
            let (texture, staging) = Self::record_dds_upload(
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
            Ok(())
        })?;

        let texture = texture_holder.expect("record_dds_upload populated texture");
        let staging = staging_holder.expect("record_dds_upload populated staging");

        // Release staging — back to pool (reuse) or destroy. Safe to
        // do here because the fence wait inside `with_one_time_commands`
        // has already returned, so the GPU is done reading the staging
        // buffer. The guard carries the buffer's create size (#4881).
        if let Some(pool) = staging_pool {
            staging.release_to(pool);
        } else {
            staging.destroy();
        }

        Ok(texture)
    }

    /// Record-only stage of a DDS upload — allocates the GPU image,
    /// allocates a staging buffer, copies CPU pixel data into staging,
    /// and RECORDS the layout-transition + copy pair into the provided
    /// command buffer. Returns the partially-built `Texture`
    /// (image + view + sampler) and the `StagingGuard` the caller MUST
    /// retain until after the submit + fence-wait completes.
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
        staging_pool: Option<&mut StagingPool>,
    ) -> Result<(Self, StagingGuard)> {
        use super::dds;

        let total_size = dds::total_data_size(meta);
        // #4511 — a returned error, not a release `assert!`: the input is
        // mod-authorable, and a truncated-but-plausible DDS must fail the
        // upload (checkerboard fallback / queued-upload drop) instead of
        // panicking the engine.
        ensure!(
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
        // #2164 / L-5 — the fresh path unwinds its own create/allocate/bind
        // window inside `create_staging_buffer`; the RAII guard covers
        // everything after, and exists BEFORE the host write (the
        // `mapped_slice_mut` failure path used to run while both the buffer
        // and the allocation were still owned by bare locals).
        let mut staging = if let Some(pool) = staging_pool {
            pool.acquire(image_size)?
        } else {
            StagingGuard::create(device, allocator, image_size, "dds_texture_staging")?
        };

        staging.mapped_slice_mut()?[..total_size as usize]
            .copy_from_slice(&pixel_data[..total_size as usize]);

        // 2. Device-local image.
        let image_flags = if meta.is_cubemap {
            vk::ImageCreateFlags::CUBE_COMPATIBLE
        } else {
            vk::ImageCreateFlags::empty()
        };
        let image_info = vk::ImageCreateInfo::default()
            .flags(image_flags)
            .image_type(vk::ImageType::TYPE_2D)
            .format(meta.format)
            .extent(vk::Extent3D {
                width: meta.width,
                height: meta.height,
                depth: 1,
            })
            .mip_levels(meta.mip_count)
            .array_layers(meta.array_layers)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = unsafe {
            // SAFETY: `device` is the caller's live logical device;
            // `image_info` is a stack-local `ImageCreateInfo` that outlives
            // this call and describes the device-local DDS texture.
            device
                .create_image(&image_info, None)
                .context("Failed to create DDS texture image")?
        };

        let image_reqs = unsafe {
            // SAFETY: pure query — `image` was just created above by this same
            // live `device` and has not been destroyed.
            device.get_image_memory_requirements(image)
        };

        let image_alloc = match allocator
            .lock()
            .expect("allocator lock poisoned")
            .allocate(&vk_alloc::AllocationCreateDesc {
                name: "dds_texture_image",
                requirements: image_reqs,
                location: MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
            })
        {
            Ok(allocation) => allocation,
            Err(error) => {
                // SAFETY: allocation failed before the image was bound or
                // referenced by a command buffer, so the unbound image can be
                // destroyed.
                unsafe { device.destroy_image(image, None) };
                return Err(error).context("Failed to allocate DDS texture image memory");
            }
        };

        // #2178 / PERF-D3-03 — free the sub-allocation on bind failure. This is
        // the most reachable instance of the pattern: unlike the startup
        // attachment allocations, this one runs per DDS upload for the whole
        // session, so a VRAM-pressure bind failure here is plausible rather
        // than hypothetical, and every failed upload would strand its
        // allocation for the process lifetime.
        if let Err(error) = unsafe {
            // SAFETY: `image` is device-created above and unbound; `image_alloc`
            // is a fresh GpuOnly allocation from this device's allocator whose
            // `memory()`/`offset()` satisfy `image`'s memory requirements
            // queried just above.
            device.bind_image_memory(image, image_alloc.memory(), image_alloc.offset())
        } {
            allocator
                .lock()
                .expect("allocator lock poisoned")
                .free(image_alloc)
                .ok();
            unsafe {
                // SAFETY: the bind failed and no view exists, so the image is
                // unreferenced and its allocation has just been returned.
                device.destroy_image(image, None);
            }
            return Err(error).context("Failed to bind DDS texture image memory");
        }

        // #4854 — create the CPU-side view before recording any commands that
        // name `image`. A queued batch may skip a failed upload and still
        // submit its command buffer, so failure cleanup must precede recording.
        let view_kind = if meta.is_cubemap {
            TextureViewKind::Cube
        } else {
            TextureViewKind::D2
        };
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(if meta.is_cubemap {
                vk::ImageViewType::CUBE
            } else {
                vk::ImageViewType::TYPE_2D
            })
            .format(meta.format)
            .subresource_range(color_subresource_mips_layers(
                meta.mip_count,
                meta.array_layers,
            ));
        let image_view = match unsafe {
            // SAFETY: `device` is live, and `view_info` describes a view of
            // the bound image and its declared mip/layer range.
            device.create_image_view(&view_info, None)
        } {
            Ok(view) => view,
            Err(error) => {
                // SAFETY: view creation failed, so nothing references the
                // image and no command buffer has recorded it yet; it is
                // destroyed before its memory is freed below.
                unsafe { device.destroy_image(image, None) };
                allocator
                    .lock()
                    .expect("allocator lock poisoned")
                    .free(image_alloc)
                    .ok();
                return Err(error).context("Failed to create DDS texture image view");
            }
        };

        // Build per-mip copy regions. The walk returns the exact byte
        // total the regions address — the same walk `dds::total_data_size`
        // prices the staging budget with, asserted here so a drift between
        // the region list and its budget can never land again (#4512).
        let (regions, regions_total) = build_dds_copy_regions(meta);
        debug_assert_eq!(
            regions_total, total_size,
            "per-mip region walk and staging budget disagree",
        );
        debug_assert!(
            regions_total <= image_size,
            "copy regions ({regions_total} B) exceed the staging buffer ({image_size} B)",
        );

        // 3-5. Record layout transitions + copy into the provided cmd.
        // Per-image barriers (not global) so multiple uploads recorded
        // into the same cmd don't serialise on each other unnecessarily.
        let barrier_to_dst =
            image_barrier_undef_to_transfer_dst_layers(image, meta.mip_count, meta.array_layers);

        // NONE as srcStageMask: UNDEFINED → TRANSFER_DST_OPTIMAL has no
        // prior writes to expose; NONE is the Vulkan 1.3 idiom
        // post-#949 / #1100 / #1122.
        unsafe {
            // SAFETY: `cmd` is a live command buffer in the recording state;
            // `image` and `staging.buffer` are device-owned and live; the
            // barrier's subresource range covers all `meta.mip_count` mips of
            // `image`, and every `regions` entry's mip/extent/offset lies
            // within `image`'s extent while `buffer_offset + mip_bytes` stays
            // within the staging buffer's `image_size`.
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::NONE,
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

        let barrier_to_read = image_barrier_transfer_dst_to_shader_read_layers(
            image,
            meta.mip_count,
            meta.array_layers,
        );

        unsafe {
            // SAFETY: `cmd` is still recording; `image` is device-owned and
            // live; `barrier_to_read`'s subresource range covers all
            // `meta.mip_count` mips of `image` and transitions the layout the
            // copy above left them in (TRANSFER_DST_OPTIMAL → shader-read).
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

        log::info!(
            "DDS texture recorded: {}x{}, {:?}, {} mips, {:?}",
            meta.width,
            meta.height,
            meta.format,
            meta.mip_count,
            view_kind,
        );

        Ok((
            Self {
                image,
                image_view,
                sampler,
                view_kind,
                allocation: Some(image_alloc),
                device: device.clone(),
                allocator: Some(allocator.clone()),
                creation_extent: vk::Extent3D {
                    width: meta.width,
                    height: meta.height,
                    depth: 1,
                },
                rgba_updateable: meta.format == vk::Format::R8G8B8A8_SRGB
                    && meta.mip_count == 1 && meta.array_layers == 1 && !meta.is_cubemap,
            },
            staging,
        ))
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
        // #2487 / D5-02 (sibling) — same nulling as `GpuBuffer::destroy`, and
        // for the same reason: `image` / `image_view` are `pub`, so a struct
        // that outlives its own `destroy()` would otherwise hand a descriptor
        // write a destroyed handle. Every current call site drops the
        // `Texture` immediately after (deferred-destroy drain, upload-failure
        // unwind, registry teardown), so this closes a latent path, not a live
        // one. `vkDestroy*` on `VK_NULL_HANDLE` is always valid, so the Drop
        // safety net below stays correct.
        self.image_view = vk::ImageView::null();
        self.image = vk::Image::null();
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
        // Skip the assert during unwind. See #1128 / REN-D4-NEW-01 + the
        // matching guard on GpuBuffer / Attachment / HistorySlot Drop impls.
        if !std::thread::panicking() {
            debug_assert!(false, "Texture leaked into Drop: call destroy() first");
        }
        unsafe {
            // SAFETY: reached only when `allocation.is_some()`, i.e. destroy()
            // was never run, so `image_view`/`image` are still live and
            // device-created. `self.device` is the stashed live logical device
            // that created them; destruction order (view before image) matches
            // the reverse of creation and neither is in use by in-flight work
            // on this drop path.
            self.device.destroy_image_view(self.image_view, None);
            self.device.destroy_image(self.image, None);
        }
        // #2487 / D5-02 (sibling) — mirrors `destroy()` so the two teardown
        // arms stay identical; redundant on this path, where nothing can read
        // the fields afterwards.
        self.image_view = vk::ImageView::null();
        self.image = vk::Image::null();
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

/// Per-mip `vk::BufferImageCopy` regions for a DDS upload, face-major
/// (all mips of +X, then -X, +Y, -Y, +Z, -Z — an ordinary 2D texture is
/// the one-layer degenerate case), together with the exact byte total
/// the regions address. The total comes from the same walk as the
/// regions, so the per-mip region list and the staging budget priced by
/// `dds::total_data_size` cannot disagree on odd sub-block final mips
/// (#4512). Byte sizes use `dds::mip_size`, so `buffer_row_length`/`
/// buffer_image_height` stay 0 (tightly packed).
fn build_dds_copy_regions(meta: &super::dds::DdsMetadata) -> (Vec<vk::BufferImageCopy>, u64) {
    use super::dds;

    let mut regions = Vec::with_capacity((meta.mip_count * meta.array_layers) as usize);
    let mut buffer_offset: u64 = 0;
    for layer in 0..meta.array_layers {
        for mip in 0..meta.mip_count {
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
                    base_array_layer: layer,
                    layer_count: 1,
                },
                image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                image_extent: vk::Extent3D {
                    width: dds::mip_dimension(meta.width, mip),
                    height: dds::mip_dimension(meta.height, mip),
                    depth: 1,
                },
            });
            buffer_offset += mip_bytes;
        }
    }
    (regions, buffer_offset)
}

/// Validate an RGBA upload (#4515): the pixel payload must be exactly `width * height * 4` bytes
/// (u64 math — the old `width * height * 4` product wraps u32), and the
/// caller's extent must match the extent the image was created with. The
/// check exists so a second consumer can rely on the documented contract;
/// mismatches fail as a returned error, not an assertion.
pub(crate) fn validate_rgba_upload(
    creation_extent: vk::Extent3D,
    width: u32,
    height: u32,
    pixel_data_len: usize,
) -> Result<()> {
    ensure!(
        width == creation_extent.width && height == creation_extent.height,
        "RGBA upload extent {width}x{height} does not match the texture's \
         creation extent {}x{}",
        creation_extent.width,
        creation_extent.height,
    );
    let expected = u64::from(width).checked_mul(u64::from(height))
        .and_then(|size| size.checked_mul(4))
        .context("RGBA upload byte size overflow")?;
    ensure!(
        pixel_data_len as u64 == expected,
        "pixel data must be width*height*4 RGBA bytes: got {pixel_data_len}, \
         expected {expected} for {width}x{height}",
    );
    Ok(())
}

/// Why a one-time submission failed, split by whether the GPU may still be
/// executing the recorded commands — which decides whether the resources
/// those commands reference may be freed (#4891).
///
/// [`with_one_time_commands`] and [`with_one_time_commands_reuse_fence`]
/// return it boxed in their `anyhow::Error`, so callers that treat every
/// failure alike are unaffected. Callers that own the resources the closure
/// recorded against (staging arenas, destination buffers or images) ask
/// [`Self::may_be_in_flight`] instead: destroy when it is `false`, leak when
/// it is `true`.
#[derive(Debug)]
pub(crate) enum OneTimeCommandError {
    /// Failed before `vkQueueSubmit` was called — allocation, begin,
    /// recording, end, or fence setup. The GPU never saw the command buffer,
    /// so everything it references may be destroyed immediately.
    NotSubmitted(anyhow::Error),
    /// `vkQueueSubmit` or the fence wait failed. After a device loss the
    /// commands may be pending, so a host-side destroy could race an
    /// in-flight transfer: the caller must keep what they reference alive.
    MaybeInFlight(anyhow::Error),
}

impl OneTimeCommandError {
    fn not_submitted(error: impl Into<anyhow::Error>, context: &'static str) -> Self {
        Self::NotSubmitted(error.into().context(context))
    }

    fn maybe_in_flight(error: impl Into<anyhow::Error>, context: &'static str) -> Self {
        Self::MaybeInFlight(error.into().context(context))
    }

    /// Whether a failed one-time submission may still be executing on the
    /// GPU. Anything that is not a [`OneTimeCommandError`] — an error the
    /// caller added before the helper ran, say — answers `true`: when the
    /// submission state is unknown, the resources must be kept.
    pub(crate) fn may_be_in_flight(error: &anyhow::Error) -> bool {
        !matches!(error.downcast_ref::<Self>(), Some(Self::NotSubmitted(_)))
    }

    fn inner(&self) -> &anyhow::Error {
        match self {
            Self::NotSubmitted(error) | Self::MaybeInFlight(error) => error,
        }
    }
}

impl std::fmt::Display for OneTimeCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.inner(), f)
    }
}

impl std::error::Error for OneTimeCommandError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner().source()
    }
}

/// Run a closure in a one-time-submit command buffer: allocate, record,
/// submit, wait, free.
///
/// The queue `Mutex` is locked for the **submit only** — it is released
/// before the fence wait, so a GPU-execution wait never serialises a second
/// graphics-queue thread (#1713; `one_time_lock_scope_tests` pins the scope
/// close between the two). Recording happens outside the lock entirely.
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
    with_one_time_commands_inner(device, queue, pool, None, f).map_err(anyhow::Error::from)
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
    with_one_time_commands_inner(device, queue, pool, Some(fence), f).map_err(anyhow::Error::from)
}

fn with_one_time_commands_inner<F>(
    device: &ash::Device,
    queue: &std::sync::Mutex<vk::Queue>,
    pool: vk::CommandPool,
    reusable_fence: Option<&std::sync::Mutex<vk::Fence>>,
    f: F,
) -> std::result::Result<(), OneTimeCommandError>
where
    F: FnOnce(vk::CommandBuffer) -> Result<()>,
{
    let alloc_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);

    let cmd = unsafe {
        // SAFETY: `device` is the caller's live logical device; `alloc_info`
        // is a stack-local `CommandBufferAllocateInfo` that outlives this call
        // and names the caller-owned, live `pool` with count 1 (so `[0]` is
        // always present).
        device
            .allocate_command_buffers(&alloc_info)
            .map_err(|e| {
                OneTimeCommandError::not_submitted(e, "Failed to allocate one-time command buffer")
            })?[0]
    };

    let begin_info =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

    unsafe {
        // SAFETY: `cmd` was just allocated above from `pool` and is in the
        // initial state (not already recording); `begin_info` is a stack-local
        // that outlives the call. `device` is live.
        //
        // #2157 — on failure `cmd` stays in the initial state (never entered
        // recording, never submitted), so freeing it directly is sound and
        // required: this is the first of the two `?` sites #1861 left leaking.
        if let Err(e) = device.begin_command_buffer(cmd, &begin_info) {
            device.free_command_buffers(pool, &[cmd]);
            return Err(OneTimeCommandError::not_submitted(
                e,
                "begin one-time command buffer",
            ));
        }
    }

    // Run the recording closure. If it fails, end + free the command buffer
    // (Vulkan spec requires end_command_buffer before free_command_buffers
    // when the buffer is in the recording state) and propagate the error
    // *without submitting*.
    if let Err(e) = f(cmd) {
        unsafe {
            // SAFETY: `cmd` is in the recording state (begun above, never
            // submitted); the spec requires ending it before free, so end then
            // free. `cmd` was allocated from `pool` and is not in flight (never
            // submitted), so freeing it here is sound. `device`/`pool` live.
            // Best-effort end; ignore the result since we're already in an
            // error path. The buffer is then freed without submission.
            let _ = device.end_command_buffer(cmd);
            device.free_command_buffers(pool, &[cmd]);
        }
        return Err(OneTimeCommandError::not_submitted(
            e,
            "one-time command recording failed; submission aborted",
        ));
    }

    unsafe {
        // SAFETY: `cmd` is in the recording state (begun above, closure
        // succeeded); `device` is live. Ending a recording buffer is the
        // required precondition before submission.
        //
        // #2157 — a failed `end_command_buffer` leaves `cmd` in the invalid
        // state, which `vkFreeCommandBuffers` accepts (it takes buffers in any
        // state except pending, and this one was never submitted). Freeing is
        // the only way to reclaim it: the second of the two `?` sites #1861
        // left leaking.
        if let Err(e) = device.end_command_buffer(cmd) {
            device.free_command_buffers(pool, &[cmd]);
            return Err(OneTimeCommandError::not_submitted(
                e,
                "end one-time command buffer",
            ));
        }
    }

    let submit_info = vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&cmd));

    unsafe {
        // SAFETY: `cmd` is a fully-recorded (ended above), never-submitted
        // buffer referenced by the stack-local `submit_info`; `device` is live;
        // the queue is externally synchronised by locking `queue` around the
        // `queue_submit` call, and the fence is either the caller's reset
        // reusable fence or a freshly created owned one. All error paths free
        // `cmd` and destroy the fence iff owned, so no handle outlives its use.
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
        // #1861 — every fallible call from here on must free `cmd` (already
        // past `end_command_buffer`, so no re-ending needed — just
        // `free_command_buffers`) and destroy the fence *if we created it*
        // before propagating, or both are leaked on an already-failing GPU
        // call (device-loss/OOM). The reusable-fence path (`owned == false`)
        // never destroys — that fence belongs to the caller.
        let (fence, owned) = match fence_guard.as_ref() {
            Some(guard) => {
                if let Err(e) = device.reset_fences(&[**guard]) {
                    device.free_command_buffers(pool, &[cmd]);
                    return Err(OneTimeCommandError::not_submitted(
                        e,
                        "reset reusable one-time fence",
                    ));
                }
                (**guard, false)
            }
            None => match device.create_fence(&vk::FenceCreateInfo::default(), None) {
                Ok(f) => (f, true),
                Err(e) => {
                    device.free_command_buffers(pool, &[cmd]);
                    return Err(OneTimeCommandError::not_submitted(
                        e,
                        "create one-time fence",
                    ));
                }
            },
        };

        // VUID-vkQueueSubmit-queue-00893 requires external synchronisation
        // of the queue for the *submit call only* — not the subsequent
        // fence wait. Scope the guard to the submit so it's released before
        // the (potentially long) GPU-execution wait; the fence + dedicated
        // command buffer still guarantee completion. Holding it across the
        // wait would serialize any future second graphics-queue thread for
        // no benefit. Bind the MutexGuard (not `*queue.lock()`, which would
        // release it end-of-statement since vk::Queue is Copy) so it
        // actually spans the submit. See CONC-D1-01 (#1713), refining
        // CONC-D2-NEW-01 (audit 2026-05-16). The reusable-`fence_guard`
        // (below) deliberately stays held across the wait — the fence must
        // not be reset/reused by another caller mid-wait.
        let submit_result = {
            let q = queue.lock().expect("graphics queue lock poisoned");
            device.queue_submit(*q, &[submit_info], fence)
        };
        if let Err(e) = submit_result {
            if owned {
                device.destroy_fence(fence, None);
            }
            drop(fence_guard);
            device.free_command_buffers(pool, &[cmd]);
            return Err(OneTimeCommandError::maybe_in_flight(
                e,
                "submit one-time commands",
            ));
        }
        if let Err(e) = device.wait_for_fences(&[fence], true, u64::MAX) {
            if owned {
                device.destroy_fence(fence, None);
            }
            drop(fence_guard);
            device.free_command_buffers(pool, &[cmd]);
            return Err(OneTimeCommandError::maybe_in_flight(
                e,
                "wait for one-time commands",
            ));
        }
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
            let checker = ((x / cell_size) + (y / cell_size)).is_multiple_of(2);
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

/// #4891 — the upload orchestrators decide destroy-vs-leak from
/// `OneTimeCommandError::may_be_in_flight`, so its classification is the
/// whole failure-path policy.
#[cfg(test)]
mod one_time_failure_class_tests {
    use super::OneTimeCommandError;
    use anyhow::Context;

    #[test]
    fn only_a_pre_submit_failure_is_safe_to_destroy() {
        let not_submitted = anyhow::Error::from(OneTimeCommandError::not_submitted(
            ash::vk::Result::ERROR_OUT_OF_DEVICE_MEMORY,
            "begin one-time command buffer",
        ));
        assert!(!OneTimeCommandError::may_be_in_flight(&not_submitted));

        let in_flight = anyhow::Error::from(OneTimeCommandError::maybe_in_flight(
            ash::vk::Result::ERROR_DEVICE_LOST,
            "wait for one-time commands",
        ));
        assert!(OneTimeCommandError::may_be_in_flight(&in_flight));

        // Context a caller layers on top must not hide the class.
        let wrapped = Err::<(), _>(not_submitted)
            .context("submit batched device-local uploads")
            .unwrap_err();
        assert!(!OneTimeCommandError::may_be_in_flight(&wrapped));

        // Unknown provenance is treated as possibly in flight: keep the
        // resources rather than risk freeing memory the GPU still reads.
        assert!(OneTimeCommandError::may_be_in_flight(&anyhow::anyhow!(
            "not a one-time submission error"
        )));
    }

    /// Both orchestrators that own recorded-against resources consult the
    /// classifier instead of applying one policy to every failure — the
    /// asymmetry #4891 reported (one always forgot, the other always
    /// destroyed).
    #[test]
    fn both_upload_orchestrators_branch_on_the_failure_class() {
        let needle = format!("OneTimeCommandError::{}(", "may_be_in_flight");
        for (name, source) in [
            (
                "vulkan/buffer.rs",
                crate::source_scan::production_text(include_str!("buffer.rs")),
            ),
            // No test module to cut — the whole file is production text.
            (
                "texture_registry/upload.rs",
                include_str!("../texture_registry/upload.rs"),
            ),
        ] {
            assert!(
                source.contains(&needle),
                "{name} must decide destroy-vs-leak from the submission state (#4891)"
            );
        }
    }
}

#[cfg(test)]
mod one_time_lock_scope_tests {
    /// CONC-D1-01 (#1713): `with_one_time_commands_inner` must release the
    /// graphics-queue Mutex BEFORE `wait_for_fences`. VUID-vkQueueSubmit-
    /// queue-00893 only requires external synchronisation of the queue for
    /// the submit call itself; holding the guard across the GPU-execution
    /// wait needlessly serializes any future second graphics-queue thread.
    /// The fence + dedicated command buffer give the completion guarantee
    /// without the lock.
    ///
    /// Static source check (no GPU device under `cargo test`), mirroring the
    /// `draw.rs::draw_frame_guards_on_empty_framebuffers_before_acquire`
    /// (#1211) precedent.
    #[test]
    fn queue_guard_released_before_one_time_fence_wait() {
        let src = crate::source_scan::production_text(include_str!("texture.rs"));
        let lock_pos = src
            .find("graphics queue lock poisoned")
            .expect("one-time helper should lock the graphics queue");
        let submit_pos = src
            .find("submit one-time commands")
            .expect("one-time helper should submit");
        let wait_pos = src
            .find("wait for one-time commands")
            .expect("one-time helper should wait on the fence");
        assert!(
            lock_pos < submit_pos && submit_pos < wait_pos,
            "expected lock -> submit -> wait ordering in with_one_time_commands_inner",
        );
        // The queue guard's scope must CLOSE between the submit and the
        // wait — i.e. a block-closing brace sits in the gap — so the wait
        // runs with the queue Mutex released. The pre-#1713 code had the
        // submit and wait as sequential statements in one block (no brace
        // between them) and would fail this check.
        let between = &src[submit_pos..wait_pos];
        assert!(
            between.contains('}'),
            "CONC-D1-01: the graphics-queue guard must be dropped (scope \
             closed) BEFORE wait_for_fences in with_one_time_commands_inner \
             — no scope close found between the submit and the wait, so the \
             Mutex is still held across the GPU wait. (#1713)",
        );
    }

    /// #1861 — every fallible call in `with_one_time_commands_inner` after
    /// the command buffer is allocated must free it before propagating an
    /// error, or a failing `create_fence` / `reset_fences` / `queue_submit`
    /// / `wait_for_fences` leaks the command buffer (and, when we own it,
    /// the fence). Pre-fix only the closure-failure branch and the
    /// success tail called `free_command_buffers`; the four later fallible
    /// calls propagated via `?` straight past it.
    ///
    /// #2157 extended it to the two sites #1861 missed —
    /// `begin_command_buffer` and `end_command_buffer`, both of which sit
    /// between the allocation and that cleanup block and still propagated
    /// via a bare `?`. Their blast radius grew when the FSR work made
    /// `FrameUpscaler::initialize_outputs` and `ExposureResource::initialize`
    /// callers that re-enter on every swapchain recreate rather than once at
    /// load time.
    ///
    /// Static source check (no GPU device under `cargo test`), same seam as
    /// `queue_guard_released_before_one_time_fence_wait` above. Counts
    /// `free_command_buffers(pool, &[cmd])` call sites in the function
    /// body: 1 (begin) + 1 (closure-failure) + 1 (end) + 4 (the four
    /// post-submit fallible calls) + 1 (success tail) = 8.
    #[test]
    fn one_time_commands_free_cmd_buffer_on_every_error_path() {
        let src = include_str!("texture.rs");
        let start = src
            .find("fn with_one_time_commands_inner")
            .expect("with_one_time_commands_inner must exist");
        let end = start
            + src[start..]
                .find("fn generate_checkerboard")
                .expect("generate_checkerboard follows with_one_time_commands_inner");
        let body = &src[start..end];

        let free_count = body.matches("free_command_buffers(pool, &[cmd])").count();
        assert_eq!(
            free_count, 8,
            "expected 8 free_command_buffers(pool, &[cmd]) call sites in \
             with_one_time_commands_inner (begin_command_buffer + \
             closure-failure + end_command_buffer + create_fence + \
             reset_fences + queue_submit + wait_for_fences error arms + the \
             success tail) — found {free_count}. A new fallible call was \
             likely added without a matching cleanup arm (#1861 / #2157).",
        );

        // Every one of the post-allocation fallible calls must be
        // followed by an explicit error arm (not a bare `?`) before its
        // `.context(...)` — pins that none of them regress back to the
        // pre-#1861 `?`-propagates-straight-through shape.
        for context_needle in [
            "begin one-time command buffer",
            "end one-time command buffer",
            "create one-time fence",
            "reset reusable one-time fence",
            "submit one-time commands",
            "wait for one-time commands",
        ] {
            let pos = body
                .find(context_needle)
                .unwrap_or_else(|| panic!("{context_needle} context string must still exist"));
            // The nearest `free_command_buffers` before this context string
            // must be closer than the nearest preceding `?` that isn't part
            // of an already-handled arm — approximated here by requiring a
            // `free_command_buffers` call within the 400 bytes preceding
            // the context string, which comfortably covers each arm's
            // `if let Err(e) = ... { destroy_fence?; drop(...)?; free_command_buffers(...); return ... }`
            // shape without reaching into a neighbouring arm.
            let window_start = pos.saturating_sub(400);
            let window = &body[window_start..pos];
            assert!(
                window.contains("free_command_buffers(pool, &[cmd])"),
                "{context_needle}: expected a free_command_buffers(pool, &[cmd]) \
                 call within the preceding error-handling arm (#1861)",
            );
        }
    }
}

/// #4511 / #4512 / #4515 — upload-path guards. The GPU-touching halves of
/// these paths can't run under `cargo test` (no Vulkan device), so the
/// behavioral pins cover the extracted pure helpers
/// (`build_dds_copy_regions`, `validate_rgba_upload`) and the remaining
/// in-`record_dds_upload` decisions are pinned as static source checks,
/// same seam as `one_time_lock_scope_tests` above.
#[cfg(test)]
mod dds_upload_guard_tests {
    use super::*;
    use crate::vulkan::dds;

    fn meta(
        width: u32,
        height: u32,
        mip_count: u32,
        array_layers: u32,
        compressed: bool,
    ) -> dds::DdsMetadata {
        dds::DdsMetadata {
            width,
            height,
            mip_count,
            format: if compressed {
                vk::Format::BC1_RGBA_UNORM_BLOCK
            } else {
                vk::Format::R8G8B8A8_UNORM
            },
            block_size: if compressed { 8 } else { 4 },
            compressed,
            array_layers,
            is_cubemap: array_layers == 6,
            data_offset: 128,
            expand: None,
        }
    }

    /// #4512 / REN-D9-2026-09-20-03 — the per-mip region walk and the
    /// `dds::total_data_size` staging budget must price identical bytes on
    /// odd sub-block final mips. The live form of the reported overrun was
    /// Skyrim deep-mip chains: a 173×2970 texture prices 349528 B at 11
    /// mips and 349536 B at 12 — exactly the audit's observed
    /// `pRegions[8] 349536 > 349528` pair (one extra 1×1 BC1 block).
    #[test]
    fn bc1_deep_chain_region_bytes_match_staging_budget() {
        for &(w, h, mips) in
            &[(344u32, 1375u32, 11u32), (173u32, 2970u32, 11u32), (173u32, 2970u32, 12u32)]
        {
            let m = meta(w, h, mips, 1, true);
            let (regions, total) = build_dds_copy_regions(&m);
            assert_eq!(regions.len() as u32, mips, "{w}x{h} @{mips}");
            assert_eq!(
                total,
                dds::total_data_size(&m),
                "{w}x{h} @{mips} mips: region walk vs staging budget",
            );

            // Contiguous offsets, whole blocks, extents matching each mip.
            let mut offset = 0u64;
            for (mip, region) in regions.iter().enumerate() {
                let mip = mip as u32;
                assert_eq!(region.buffer_offset, offset, "{w}x{h} mip {mip}");
                assert_eq!(region.image_extent.width, dds::mip_dimension(w, mip));
                assert_eq!(region.image_extent.height, dds::mip_dimension(h, mip));
                offset += dds::mip_size(w, h, mip, 8, true);
            }
            assert_eq!(offset, total, "{w}x{h} @{mips} mips: per-region sum");
        }
        // The observed pair, pinned as literals: 11 mips vs 12 differ by
        // exactly one BC1 block — both must price the same on both sides.
        assert_eq!(
            dds::total_data_size(&meta(173, 2970, 11, 1, true)),
            349_528u64,
        );
        assert_eq!(
            dds::total_data_size(&meta(173, 2970, 12, 1, true)),
            349_536u64,
        );
    }

    #[test]
    fn cubemap_regions_cover_all_six_faces() {
        let m = meta(4, 4, 1, 6, true);
        let (regions, total) = build_dds_copy_regions(&m);
        assert_eq!(regions.len(), 6);
        assert_eq!(total, 48); // 6 faces × one 4×4 BC1 block (8 B)
        assert_eq!(total, dds::total_data_size(&m));
        for (layer, region) in regions.iter().enumerate() {
            assert_eq!(region.image_subresource.base_array_layer, layer as u32);
        }
    }

    /// #4515 / REN-D5-2026-09-20-02 — a streaming upload whose extent
    /// differs from the texture's creation extent must be an error, and
    /// the payload-length check must survive the u32 wrap
    /// (`65536² × 4 = 2^34`).
    #[test]
    fn rgba_overwrite_extent_mismatch_is_an_error() {
        let creation = vk::Extent3D {
            width: 256,
            height: 128,
            depth: 1,
        };
        assert!(validate_rgba_upload(creation, 256, 128, 256 * 128 * 4).is_ok());
        // A second consumer relying on the old (nonexistent) assertion
        // would upload a larger overlay into the smaller image.
        let err = validate_rgba_upload(creation, 512, 256, 512 * 256 * 4)
            .expect_err("extent mismatch must fail");
        assert!(err.to_string().contains("creation extent"), "{err}");
    }

    #[test]
    fn rgba_overwrite_payload_length_uses_u64_math() {
        let creation = vk::Extent3D {
            width: 65536,
            height: 65536,
            depth: 1,
        };
        let err = validate_rgba_upload(creation, 65536, 65536, 0)
            .expect_err("wrong payload length must fail");
        assert!(err.to_string().contains("expected"), "{err}");
        // 65536² × 4 = 2^34: the old u32 `width * height * 4` wrapped to 0.
        assert!(
            validate_rgba_upload(creation, 65536, 65536, 17_179_869_184usize).is_ok()
        );
    }

    /// #4511 — the payload-length gate in `record_dds_upload` must be a
    /// returned error (`ensure!`), not a release `assert!`: a
    /// truncated-but-plausible DDS has to fail the upload (checkerboard
    /// fallback / queued-upload drop) rather than panic the engine.
    #[test]
    fn dds_payload_check_is_an_error_not_a_release_assert() {
        let src = include_str!("texture.rs");
        let pos = src
            .find("DDS pixel data too small")
            .expect("payload-length gate message must exist");
        let pre = &src[pos.saturating_sub(160)..pos];
        assert!(
            pre.contains("ensure!("),
            "the DDS payload gate must be anyhow ensure! (returned error); \
             found: {pre}",
        );
        assert!(
            !pre.contains("assert!("),
            "the DDS payload gate regressed to a release assert!: {pre}",
        );
    }

    /// #4854 — allocation/view creation must clean up while the image has
    /// not yet been named by this batch's command buffer. A view failure is
    /// recoverable per queued upload, so the batch may still submit.
    #[test]
    fn dds_image_setup_failures_unwind_before_recording_copy_commands() {
        let src = crate::source_scan::production_text(include_str!("texture.rs"));
        let upload_at = src
            .find("fn record_dds_upload(")
            .expect("record_dds_upload production method");
        let upload = &src[upload_at..src[upload_at..]
            .find("/// Destroy the texture")
            .map(|n| upload_at + n)
            .expect("record_dds_upload method end")];
        let create_view = upload
            .find("device.create_image_view(&view_info, None)")
            .expect("image view creation");
        let record_barrier = upload
            .find("device.cmd_pipeline_barrier(")
            .expect("first image transition command");
        assert!(create_view < record_barrier);
        let view_failure = &upload[create_view..record_barrier];
        assert!(view_failure.contains("device.destroy_image(image, None)"));
        assert!(view_failure.contains(".free(image_alloc)"));

        let allocation = upload
            .find("Failed to allocate DDS texture image memory")
            .expect("image allocation error context");
        let allocation_failure = &upload[allocation.saturating_sub(260)..allocation + 80];
        assert!(allocation_failure.contains("device.destroy_image(image, None)"));

        // #4880 (REN-D5-2026-09-26-02b) — the other half of the batched-flush
        // hazard: once a copy from the staging buffer is recorded, a
        // mid-record early-return would drop the StagingGuard (destroying the
        // buffer) while `flush_upload_batch` still submits the command
        // buffer — a copy from freed memory (VUID-vkQueueSubmit-
        // pCommandBuffers-00070 class). Pin that the recording tail after the
        // first `cmd_pipeline_barrier` contains no fallible `?` at all, so
        // nothing can fail between record and Ok. (`{:?}` in the completion
        // log's format string is the one tolerated spelling.)
        let tail = &upload[record_barrier..];
        let tail_without_format_specs = tail.replace("{:?}", "");
        assert!(
            !tail_without_format_specs.contains('?'),
            "record_dds_upload grew a fallible operation after the first \
             recorded command — a failure there would submit the batched \
             command buffer with a copy from a dropped staging buffer \
             (#4880). Move the failure point back before the view/barrier \
             setup, alongside the #2178/#4854 unwinds.",
        );
    }

    /// #4512 / #4881 — the staging release label comes from the guard
    /// (the VkBuffer create size), not from a size this function computes:
    /// the allocation footprint over-labels (+8 B `vkCmdCopyBufferToImage`
    /// overruns, #4512) and the request size under-labels a reused buffer
    /// (unbounded retained memory, #4881).
    #[test]
    fn dds_upload_does_not_compute_its_own_staging_release_label() {
        let production = crate::source_scan::production_text(include_str!("texture.rs"));
        assert!(!production.contains("staging_capacity"));
        assert!(!production.contains("allocation.size()"));
        assert!(production.contains("staging.release_to(pool);"));
    }

    /// #4515 — `Texture` must store its creation extent and the in-place
    /// RGBA gate must compare uploads against it (the doc previously claimed
    /// an assertion that did not exist). #4892 deleted the one-shot
    /// `overwrite_rgba_pixels` this used to pin; `can_update_rgba` is the
    /// gate every queued replacement (`write_rgba_inplace`, `update_rgba`,
    /// and the record-time recheck) now goes through.
    #[test]
    fn rgba_overwrite_validates_against_creation_extent() {
        let src = crate::source_scan::production_text(include_str!("texture.rs"));
        assert!(
            src.contains("creation_extent: vk::Extent3D"),
            "Texture must store its creation extent (#4515)",
        );
        let fn_pos = src
            .find("fn can_update_rgba")
            .expect("can_update_rgba must exist");
        let body = &src[fn_pos..fn_pos + src[fn_pos..]
            .find("\n    }\n")
            .expect("can_update_rgba closes at impl indentation")];
        assert!(
            body.contains("self.creation_extent.width == width")
                && body.contains("self.creation_extent.height == height"),
            "can_update_rgba must check the upload against the texture's \
             stored creation extent (#4515)",
        );
    }
}
