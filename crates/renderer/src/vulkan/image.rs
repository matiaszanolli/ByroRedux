//! `GpuImage` — the image-side analogue of [`GpuBuffer`] (#3860).
//!
//! Every screen-sized GPU image in this renderer was built by the same
//! five-step sequence — `vk::ImageCreateInfo` → `create_image` →
//! `allocator.allocate` → `bind_image_memory` → `create_image_view` — with the
//! same three-arm error cleanup, copied into fourteen files. `buffer.rs`
//! solved the identical problem on the buffer side years earlier
//! (`GpuBuffer::create_vertex_buffer`, `create_host_visible`, …); the image
//! side never got the analogue.
//!
//! The cost was not the ~80 lines per site. It was that the *cleanup ordering
//! and the allocator-lock scope* had to be re-derived at each one, and the
//! same defect was consequently fixed four separate times, once per copy:
//! #1163 (allocator `MutexGuard` held across an error arm that re-locks),
//! #1164 (bind ordering, so the partial-state invariant is structural), #1165
//! ("deadlock identical to #1163", in a different file), and #2178
//! (sub-allocation stranded on bind failure) — whose own comment records the
//! author hand-checking three sibling copies. Two files still carry comments
//! reading *"Cf. ssao.rs for the #1163 separate-let pattern"*.
//!
//! Those rules now live here once:
//! - the allocator lock is taken, used, and released in a single statement,
//!   never held across a fallible call that might re-lock it (#1163 / #1165);
//! - on allocate failure the image is destroyed and nothing was bound;
//! - on bind or view failure the allocation is freed **before** the image is
//!   destroyed, so no sub-allocation is stranded (#2178);
//! - and a `Drop` safety net mirrors `GpuBuffer`'s (#656) for anything that
//!   escapes the canonical `destroy()` path.
//!
//! [`GpuBuffer`]: super::buffer::GpuBuffer

use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;

use super::allocator::SharedAllocator;

/// What to build. Everything the fourteen migrated sites vary; everything
/// they had in common is applied by [`GpuImage::create`] and is not
/// expressible here — 1 mip level, `SampleCountFlags::TYPE_1`,
/// `ImageTiling::OPTIMAL`, `SharingMode::EXCLUSIVE`, and an initial layout of
/// `UNDEFINED`. Those were identical at every site, so they are the
/// helper's contract rather than parameters; a site that needs to differ
/// should say so loudly by not using this type.
#[derive(Debug, Clone, Copy)]
pub struct GpuImageDesc<'a> {
    /// Allocation label, also used verbatim in every error context
    /// (`create {name}` / `allocate {name}` / `bind {name}` / `view {name}`),
    /// matching the message shapes the copied sites already used.
    pub name: &'a str,
    pub extent: vk::Extent3D,
    pub format: vk::Format,
    pub usage: vk::ImageUsageFlags,
    pub image_type: vk::ImageType,
    pub view_type: vk::ImageViewType,
    pub aspect: vk::ImageAspectFlags,
    pub array_layers: u32,
}

impl<'a> GpuImageDesc<'a> {
    /// The overwhelmingly common case: a single-layer 2D colour image.
    pub fn color_2d(
        name: &'a str,
        width: u32,
        height: u32,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
    ) -> Self {
        Self {
            name,
            extent: vk::Extent3D {
                width,
                height,
                depth: 1,
            },
            format,
            usage,
            image_type: vk::ImageType::TYPE_2D,
            view_type: vk::ImageViewType::TYPE_2D,
            aspect: vk::ImageAspectFlags::COLOR,
            array_layers: 1,
        }
    }

    /// A layered 2D colour image viewed as `TYPE_2D_ARRAY`
    /// (`groundcover_bench.rs`).
    pub fn color_2d_array(
        name: &'a str,
        width: u32,
        height: u32,
        layers: u32,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
    ) -> Self {
        Self {
            view_type: vk::ImageViewType::TYPE_2D_ARRAY,
            array_layers: layers,
            ..Self::color_2d(name, width, height, format, usage)
        }
    }

    /// A 3D colour volume (`volumetrics/init.rs`'s froxel grid).
    pub fn color_3d(
        name: &'a str,
        extent: vk::Extent3D,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
    ) -> Self {
        Self {
            extent,
            image_type: vk::ImageType::TYPE_3D,
            view_type: vk::ImageViewType::TYPE_3D,
            ..Self::color_2d(name, extent.width, extent.height, format, usage)
        }
    }

    /// A 2D depth image — the one site whose view aspect is `DEPTH` rather
    /// than `COLOR` (`context/helpers.rs`).
    pub fn depth_2d(
        name: &'a str,
        width: u32,
        height: u32,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
    ) -> Self {
        Self {
            aspect: vk::ImageAspectFlags::DEPTH,
            ..Self::color_2d(name, width, height, format, usage)
        }
    }
}

/// An owned device-local image with its bound allocation and its view.
///
/// Field layout mirrors [`GpuBuffer`](super::buffer::GpuBuffer) deliberately,
/// including the stashed `device` / `allocator` clones that make the `Drop`
/// safety net possible: `ash::Device` is `Arc`-backed and `SharedAllocator` is
/// already an `Arc<Mutex<…>>`, so both clones are cheap.
pub struct GpuImage {
    pub image: vk::Image,
    pub view: vk::ImageView,
    allocation: Option<vk_alloc::Allocation>,
    /// Stashed at construction so `Drop` can self-free when the canonical
    /// `destroy(device, allocator)` path is missed.
    device: ash::Device,
    /// `Option` for the #927 reason `GpuBuffer` documents: `destroy()`
    /// releases this Arc clone immediately, so it cannot still be
    /// outstanding when `VulkanContext::Drop` reaches its `Arc::try_unwrap`
    /// and takes the #665 leak-guard branch.
    allocator: Option<SharedAllocator>,
}

impl GpuImage {
    /// Create → allocate → bind → view, with the cleanup ordering that four
    /// separate issues had to establish independently in four copies.
    pub fn create(
        device: &ash::Device,
        allocator: &SharedAllocator,
        desc: &GpuImageDesc<'_>,
    ) -> Result<Self> {
        let name = desc.name;
        let create_info = vk::ImageCreateInfo::default()
            .image_type(desc.image_type)
            .format(desc.format)
            .extent(desc.extent)
            .mip_levels(1)
            .array_layers(desc.array_layers)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(desc.usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        // SAFETY: `create_info` is fully populated above. On Ok the handle is
        // owned by the `GpuImage` returned below; on Err the `?` bubbles out
        // before anything is allocated or bound.
        let image = unsafe {
            device
                .create_image(&create_info, None)
                .with_context(|| format!("create {name}"))?
        };

        // SAFETY: `image` was just created; the handle is live.
        let requirements = unsafe { device.get_image_memory_requirements(image) };
        // #1163 / #1165 — the lock is acquired, used and dropped inside this
        // one statement. Binding the guard to a `let` and holding it across
        // the error arms below deadlocks, because those arms re-lock to free.
        let allocation = match allocator
            .lock()
            .expect("allocator lock")
            .allocate(&vk_alloc::AllocationCreateDesc {
                name,
                requirements,
                location: gpu_allocator::MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
            })
            .with_context(|| format!("allocate {name}"))
        {
            Ok(allocation) => allocation,
            Err(e) => {
                // SAFETY: created above, never bound, no other reference.
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };

        // SAFETY: `image` produced `requirements`, which produced
        // `allocation`; bound exactly once.
        if let Err(e) = unsafe {
            device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .with_context(|| format!("bind {name}"))
        } {
            // #2178 — free the sub-allocation BEFORE destroying the image, or
            // the slab is stranded for the process lifetime.
            Self::free_allocation(allocator, allocation, name);
            // SAFETY: the allocation is already freed, so the image is not
            // bound to live memory.
            unsafe { device.destroy_image(image, None) };
            return Err(e);
        }

        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(desc.view_type)
            .format(desc.format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: desc.aspect,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: desc.array_layers,
            });
        // SAFETY: `image` is bound (above); the view is owned by the returned
        // `GpuImage` on Ok.
        let view = match unsafe {
            device
                .create_image_view(&view_info, None)
                .with_context(|| format!("view {name}"))
        } {
            Ok(view) => view,
            Err(e) => {
                Self::free_allocation(allocator, allocation, name);
                // SAFETY: allocation freed first, same ordering as the bind
                // arm. No view exists on this path.
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };

        Ok(Self {
            image,
            view,
            allocation: Some(allocation),
            device: device.clone(),
            allocator: Some(allocator.clone()),
        })
    }

    /// Destroy the view and image and release the allocation.
    ///
    /// Idempotent: a second call is a no-op, and it disarms [`Drop`]'s safety
    /// net. Callers pass `device`/`allocator` explicitly to match every
    /// existing teardown site's shape, though the stashed clones would do.
    pub fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        if self.allocation.is_none() {
            return;
        }
        // SAFETY: caller's contract for every `destroy` in this renderer —
        // the device is idle (`VulkanContext::Drop` waits first), so no
        // in-flight command buffer references the view or image.
        unsafe {
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
        }
        self.view = vk::ImageView::null();
        self.image = vk::Image::null();
        if let Some(allocation) = self.allocation.take() {
            Self::free_allocation(allocator, allocation, "GpuImage::destroy");
        }
        // #927 — release the Arc clone now, not when the struct drops, so it
        // cannot be outstanding at `VulkanContext::Drop`'s `Arc::try_unwrap`.
        self.allocator = None;
    }

    /// One place that takes the allocator lock to free, so no error arm can
    /// re-derive the lock scope differently (#1163).
    fn free_allocation(allocator: &SharedAllocator, allocation: vk_alloc::Allocation, name: &str) {
        match allocator.lock() {
            Ok(mut guard) => {
                if let Err(e) = guard.free(allocation) {
                    log::error!("GpuImage failed to free allocation for {name}: {e}");
                }
            }
            Err(poisoned) => {
                if let Err(e) = poisoned.into_inner().free(allocation) {
                    log::error!("GpuImage failed to free allocation for {name}: {e}");
                }
            }
        }
    }
}

impl Drop for GpuImage {
    /// Safety net mirroring `GpuBuffer::Drop` (#656) and `Texture::Drop`.
    ///
    /// When the canonical `destroy(device, allocator)` ran, `allocation` is
    /// `None` and this is a no-op. When something escapes that path — a panic
    /// mid-construction, an ad-hoc value, a future teardown that forgets a
    /// field — the image, view and slab are still reclaimed instead of leaking
    /// in release builds.
    fn drop(&mut self) {
        if self.allocation.is_none() {
            return;
        }
        log::warn!("GpuImage dropped without destroy() — running cleanup from Drop (#3860)");
        // Skip the assert during unwind, so a panic partway through a
        // multi-image init does not compound into N nested debug_assert
        // panics that clobber the original one (#1128 / REN-D4-NEW-01).
        if !std::thread::panicking() {
            debug_assert!(false, "GpuImage leaked into Drop: call destroy() first");
        }
        // SAFETY: this arm runs only while `allocation` is still `Some`, i.e.
        // `destroy()` never ran, so both handles are live and were created by
        // `self.device`, which outlives the safety-net path.
        unsafe {
            self.device.destroy_image_view(self.view, None);
            self.device.destroy_image(self.image, None);
        }
        self.view = vk::ImageView::null();
        self.image = vk::Image::null();
        if let Some(allocation) = self.allocation.take() {
            let Some(allocator) = self.allocator.as_ref() else {
                log::error!(
                    "GpuImage::Drop has live allocation but no allocator — slab leaks \
                     (was destroy() partially invoked?)",
                );
                return;
            };
            Self::free_allocation(allocator, allocation, "GpuImage::Drop");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The descriptor constructors are pure, so the part of this helper that
    /// callers get wrong most easily — which view type and aspect go with
    /// which image type — is testable without a device.
    #[test]
    fn the_descriptor_constructors_pair_view_type_with_image_type() {
        let d = GpuImageDesc::color_2d(
            "t",
            4,
            2,
            vk::Format::R8_UNORM,
            vk::ImageUsageFlags::empty(),
        );
        assert_eq!(d.image_type, vk::ImageType::TYPE_2D);
        assert_eq!(d.view_type, vk::ImageViewType::TYPE_2D);
        assert_eq!(d.aspect, vk::ImageAspectFlags::COLOR);
        assert_eq!(d.array_layers, 1);
        assert_eq!(d.extent.depth, 1, "a 2D image is depth 1, not depth 0");

        let a = GpuImageDesc::color_2d_array(
            "t",
            4,
            2,
            6,
            vk::Format::R8_UNORM,
            vk::ImageUsageFlags::empty(),
        );
        assert_eq!(a.image_type, vk::ImageType::TYPE_2D, "layered, still 2D");
        assert_eq!(a.view_type, vk::ImageViewType::TYPE_2D_ARRAY);
        assert_eq!(a.array_layers, 6);

        let extent = vk::Extent3D {
            width: 8,
            height: 4,
            depth: 16,
        };
        let v = GpuImageDesc::color_3d(
            "t",
            extent,
            vk::Format::R16G16B16A16_SFLOAT,
            vk::ImageUsageFlags::STORAGE,
        );
        assert_eq!(v.image_type, vk::ImageType::TYPE_3D);
        assert_eq!(v.view_type, vk::ImageViewType::TYPE_3D);
        assert_eq!(v.extent.depth, 16, "the froxel grid's depth must survive");
        assert_eq!(
            v.array_layers, 1,
            "a 3D image has one layer, not `depth` layers"
        );

        let z = GpuImageDesc::depth_2d(
            "t",
            4,
            2,
            vk::Format::D32_SFLOAT,
            vk::ImageUsageFlags::empty(),
        );
        assert_eq!(
            z.aspect,
            vk::ImageAspectFlags::DEPTH,
            "a depth image's view aspect must be DEPTH — a COLOR aspect here is a \
             validation error the type system cannot catch"
        );
    }

    /// #3860 — the consolidation, enforced.
    ///
    /// The finding is not that the chain was long; it is that a fix to its
    /// cleanup ordering had fourteen landing sites and no compiler help
    /// finding them, which is how #1163 / #1164 / #1165 / #2178 came to be
    /// four fixes for one defect. That only stays fixed if new render passes
    /// cannot quietly re-roll the chain, so: `bind_image_memory` may appear in
    /// exactly two files.
    #[test]
    fn no_file_outside_this_module_rolls_its_own_image_chain() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/vulkan");
        // Relative paths, not bare file names: `init.rs` and `helpers.rs` each
        // exist in more than one directory under `src/vulkan`, so matching on
        // the basename would exempt the wrong file.
        //
        // `image.rs` is the helper; `texture.rs` is a genuine specialisation
        // rather than a copy — it uploads host data, generates mip chains and
        // drives its own layout transitions, none of which `GpuImage` models.
        const ALLOWED: &[&str] = &["image.rs", "texture.rs"];
        // The #3860 ledger: every entry is a site still to migrate. Each
        // migration commit deletes exactly one line, so the remaining work is
        // countable from the source and cannot be quietly abandoned half-done.
        // The second assertion fails on a stale entry, so the list cannot rot
        // into a blanket exemption. When it empties, delete it and that check.
        const PENDING: &[&str] = &[
            "bloom.rs",
            "caustic.rs",
            "composite.rs",
            "context/helpers.rs",
            "exposure.rs",
            "frame_upscaler.rs",
            "gbuffer.rs",
            "groundcover_bench.rs",
            "placeholder.rs",
            "ssao.rs",
            "svgf.rs",
            "volumetrics/init.rs",
            "water_caustic.rs",
        ];

        let mut offenders = Vec::new();
        let mut pending_still_rolling = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("renderer src/vulkan must be readable") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if ALLOWED.contains(&rel.as_str()) {
                    continue;
                }
                if !std::fs::read_to_string(&path)
                    .expect("read source")
                    .contains("bind_image_memory")
                {
                    continue;
                }
                if PENDING.contains(&rel.as_str()) {
                    pending_still_rolling.push(rel);
                } else {
                    offenders.push(rel);
                }
            }
        }

        offenders.sort();
        assert!(
            offenders.is_empty(),
            "these files hand-roll the create → allocate → bind → view chain instead of \
             using `GpuImage::create`: {offenders:?}. That chain's cleanup ordering and \
             allocator-lock scope were re-derived at every copy, and the same defect was \
             fixed four separate times as a result (#1163, #1164, #1165, #2178). If a site \
             genuinely cannot use `GpuImage` (as `texture.rs` cannot — mip generation and \
             layout transitions), add it to ALLOWED with the reason."
        );

        pending_still_rolling.sort();
        let mut expected: Vec<String> = PENDING.iter().map(|s| (*s).to_owned()).collect();
        expected.sort();
        assert_eq!(
            pending_still_rolling, expected,
            "the PENDING ledger is stale. A listed file no longer hand-rolls the chain \
             (delete its line — that is the migration commit's job) or no longer exists \
             at that path. A stale entry is a silent hole in the gate above."
        );
    }
}
