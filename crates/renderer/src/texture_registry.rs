//! Bindless texture registry — all textures in a single descriptor array.
//!
//! Instead of per-texture descriptor sets, all textures live in a global
//! `sampler2D textures[]` array (Vulkan descriptor indexing). The draw
//! loop binds this once per frame; the fragment shader indexes into it
//! via a per-instance `texture_index` from the instance SSBO.
//!
//! One descriptor set per frame-in-flight. Writes race carefully:
//! updates (`update_rgba`, `drop_texture` fallback redirect, texture
//! registration) target ONLY the current recording slot's set
//! immediately, and queue the same write on each other slot. The queue
//! is flushed in [`TextureRegistry::begin_frame`] once the caller has
//! already waited on the new slot's fence — guaranteeing no in-flight
//! command buffer still references the set being updated, per
//! VUID-vkUpdateDescriptorSets-None-03047. See #92.

use crate::vulkan::allocator::SharedAllocator;
use crate::vulkan::buffer::StagingPool;
use crate::vulkan::texture::Texture;
use anyhow::{Context, Result};
use ash::vk;
use std::collections::{HashMap, VecDeque};

/// Handle into the TextureRegistry (index into the bindless array).
pub type TextureHandle = u32;

/// Maximum frames in flight — textures must survive this many frames after replacement.
const MAX_FRAMES_IN_FLIGHT: usize = 2;

/// One queued `vkUpdateDescriptorSets` payload for a slot whose
/// descriptor set is not currently being recorded. The write is
/// replayed on the slot's next `begin_frame` after its fence
/// signals. See #92.
#[derive(Debug, Clone, Copy)]
struct PendingSetWrite {
    handle: TextureHandle,
    image_view: vk::ImageView,
    sampler: vk::Sampler,
}

struct TextureEntry {
    /// Live texture, or `None` after the handle has been dropped via
    /// [`TextureRegistry::drop_texture`]. Bindless indexing still works
    /// for dropped handles: they are redirected to the fallback
    /// checkerboard so a stale draw call degrades gracefully instead of
    /// reading a freed `VkImageView`. See #372.
    texture: Option<Texture>,
    /// Ring of replaced / dropped textures awaiting deferred destruction.
    /// See issues #134, #372.
    pending_destroy: VecDeque<(u64, Texture)>,
    /// Live reference count. Incremented on every acquisition (initial
    /// upload, `load_dds` cache hit, `acquire_by_path` cache hit) and
    /// decremented by `drop_texture`. The GPU resource is freed only
    /// when the count reaches zero. Without this, cell A's unload
    /// would free textures still in use by a still-resident cell B
    /// once M40 doorwalking lands. See #524.
    ///
    /// Invariant: `texture.is_some() iff ref_count > 0`.
    ref_count: u32,
}

/// Bindless texture registry.
///
/// All textures are stored in a `sampler2D textures[max_textures]` descriptor
/// array. Two copies exist (per frame-in-flight) for safe descriptor updates.
pub struct TextureRegistry {
    textures: Vec<TextureEntry>,
    path_map: HashMap<String, TextureHandle>,
    fallback_handle: TextureHandle,
    descriptor_pool: vk::DescriptorPool,
    /// Layout for set 0: binding 0 = sampler2D[max_textures], PARTIALLY_BOUND.
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    /// One bindless descriptor set per frame-in-flight.
    bindless_sets: Vec<vk::DescriptorSet>,
    /// Shared sampler for all textures.
    pub shared_sampler: vk::Sampler,
    max_textures: u32,
    /// Monotonic frame counter for deferred-destroy aging (issue #134).
    current_frame_id: u64,
    /// Retained staging buffers for texture uploads (#239). Reuses
    /// staging memory across `load_dds` / `register_rgba` calls
    /// instead of hitting gpu-allocator per-texture. See
    /// `vulkan::buffer::DEFAULT_STAGING_BUDGET_BYTES` for the
    /// retention cap. `destroy()` tears this down alongside the
    /// descriptor pool + sampler.
    ///
    /// `Option` so unit tests can build a registry without a real
    /// Vulkan device/allocator (StagingPool requires both). Production
    /// construction via [`TextureRegistry::new`] always sets this.
    staging_pool: Option<StagingPool>,
    /// Descriptor-set writes queued per frame slot. Written to the
    /// current slot (`current_slot`) immediately; queued on every
    /// other slot so the write doesn't race with a still-in-flight
    /// command buffer referencing that slot's set. Flushed in
    /// `begin_frame` after the caller waits on the new slot's fence.
    /// See #92 / VUID-vkUpdateDescriptorSets-None-03047.
    pending_set_writes: Vec<Vec<PendingSetWrite>>,
    /// Frame-in-flight slot index (`0..MAX_FRAMES_IN_FLIGHT`) currently
    /// being recorded by the CPU. Updated by `begin_frame`. Immediate
    /// descriptor writes target `bindless_sets[current_slot]`; writes
    /// for every other slot go into `pending_set_writes[other]`.
    current_slot: usize,
}

impl TextureRegistry {
    /// Reject a registration that would exceed the bindless array bound.
    ///
    /// Before #425 the array was sized to a hardcoded 1024 and callers
    /// would silently write past the bound once a cell loaded more unique
    /// textures, producing corrupted descriptor state or driver crashes.
    /// Now `max_textures` is driven by the device's
    /// `maxPerStageDescriptorUpdateAfterBindSampledImages` limit (clamped
    /// at the R16_UINT mesh-id ceiling), and this check returns an error
    /// when the slot count is truly exhausted — the caller
    /// (`asset_provider::resolve_texture`) already falls back to the
    /// checkerboard handle on `Err`, so overflow degrades gracefully.
    fn check_slot_available(&self) -> Result<()> {
        if self.textures.len() as u32 >= self.max_textures {
            anyhow::bail!(
                "TextureRegistry is full: {} of {} bindless slots in use — raise the device's maxPerStageDescriptorUpdateAfterBindSampledImages limit or reduce unique texture count (#425)",
                self.textures.len(),
                self.max_textures
            );
        }
        Ok(())
    }

    /// Create a new bindless texture registry.
    ///
    /// Requires `descriptorBindingPartiallyBound` and `runtimeDescriptorArray`
    /// to be enabled on the device (Vulkan 1.2 core features).
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        _swapchain_image_count: u32, // unused with bindless — kept for API compat
        max_textures: u32,
        max_sampler_anisotropy: f32,
    ) -> Result<Self> {
        // Descriptor set layout: binding 0 = sampler2D[max_textures].
        // PARTIALLY_BOUND allows uninitialized array elements (the shader
        // only accesses indices that correspond to loaded textures).
        // UPDATE_AFTER_BIND allows writing new texture descriptors to a set
        // while a prior frame's command buffer still references it — safe
        // because only previously-unbound array indices are written.
        let binding_flags = [vk::DescriptorBindingFlags::PARTIALLY_BOUND
            | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND];
        let mut binding_flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&binding_flags);

        let binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(max_textures)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);

        // Validate against every shader that consumes the bindless texture
        // array — triangle.frag, ui.frag, composite.vert/frag (#427).
        // Composite references the bindless array via set=1 in the pipeline
        // layout, but validation here asserts set=0 agreement with triangle/ui.
        crate::vulkan::reflect::validate_set_layout(
            0,
            std::slice::from_ref(&binding),
            &[
                crate::vulkan::reflect::ReflectedShader {
                    name: "triangle.frag",
                    spirv: crate::vulkan::pipeline::TRIANGLE_FRAG_SPV,
                },
                crate::vulkan::reflect::ReflectedShader {
                    name: "ui.frag",
                    spirv: crate::vulkan::pipeline::UI_FRAG_SPV,
                },
            ],
            "bindless textures (set=0)",
            &[],
        )
        .expect("bindless texture layout drifted against triangle/ui frag shaders (see #427)");

        let layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
            .bindings(std::slice::from_ref(&binding))
            .push_next(&mut binding_flags_info);

        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(&layout_info, None)
                .context("Failed to create bindless texture descriptor set layout")?
        };

        // Pool: 2 sets (per frame-in-flight), each with max_textures samplers.
        let pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: max_textures * MAX_FRAMES_IN_FLIGHT as u32,
        };
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND)
            .pool_sizes(std::slice::from_ref(&pool_size))
            .max_sets(MAX_FRAMES_IN_FLIGHT as u32);

        let descriptor_pool = unsafe {
            device
                .create_descriptor_pool(&pool_info, None)
                .context("Failed to create bindless texture descriptor pool")?
        };

        // Allocate per-frame-in-flight descriptor sets.
        let layouts = vec![descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);
        let bindless_sets = unsafe {
            device
                .allocate_descriptor_sets(&alloc_info)
                .context("Failed to allocate bindless texture descriptor sets")?
        };

        // Shared sampler: LINEAR/REPEAT with anisotropic filtering.
        let anisotropy_enable = max_sampler_anisotropy > 1.0;
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .anisotropy_enable(anisotropy_enable)
            .max_anisotropy(if anisotropy_enable {
                max_sampler_anisotropy
            } else {
                1.0
            })
            .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
            .unnormalized_coordinates(false)
            .compare_enable(false)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .min_lod(0.0)
            .max_lod(16.0);

        let shared_sampler = unsafe {
            device
                .create_sampler(&sampler_info, None)
                .context("Failed to create shared texture sampler")?
        };

        log::info!(
            "TextureRegistry created: bindless array[{}] × {} frames, anisotropy {}",
            max_textures,
            MAX_FRAMES_IN_FLIGHT,
            if anisotropy_enable {
                format!("{:.0}×", max_sampler_anisotropy)
            } else {
                "disabled".to_string()
            },
        );

        // Staging pool owns cloned device + allocator handles so that
        // texture uploads amortize the gpu-allocator bookkeeping across
        // a burst. Default 128 MB retained cap — see #239 / #511.
        let staging_pool = Some(StagingPool::new(device.clone(), allocator.clone()));

        Ok(Self {
            textures: Vec::new(),
            path_map: HashMap::new(),
            fallback_handle: 0,
            descriptor_pool,
            descriptor_set_layout,
            bindless_sets,
            shared_sampler,
            max_textures,
            current_frame_id: 0,
            staging_pool,
            pending_set_writes: vec![Vec::new(); MAX_FRAMES_IN_FLIGHT],
            current_slot: 0,
        })
    }

    /// Register the fallback texture as handle 0. Must be called once after new().
    pub fn set_fallback(&mut self, device: &ash::Device, fallback_texture: Texture) -> Result<()> {
        let handle = self.textures.len() as TextureHandle;
        self.write_texture_to_all_sets(device, handle, &fallback_texture);
        // Fallback is special: `drop_texture` is a no-op for it at the
        // call site (cell_loader filters `th.0 != fallback`), so it
        // never decrements. Start at `u32::MAX` so any stray decrement
        // can't underflow into a freed state.
        self.textures.push(TextureEntry {
            texture: Some(fallback_texture),
            pending_destroy: VecDeque::new(),
            ref_count: u32::MAX,
        });
        self.fallback_handle = handle;
        Ok(())
    }

    /// Load a DDS texture from raw bytes, or return a cached handle if already loaded.
    ///
    /// Both the initial upload and a cache hit bump the entry's
    /// reference count. Pair each call with a matching `drop_texture`.
    /// See #524.
    pub fn load_dds(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        path: &str,
        dds_bytes: &[u8],
    ) -> Result<TextureHandle> {
        let normalized = normalize_path(path);

        if let Some(&handle) = self.path_map.get(&normalized) {
            if let Some(entry) = self.textures.get_mut(handle as usize) {
                entry.ref_count = entry.ref_count.saturating_add(1);
            }
            return Ok(handle);
        }

        // Reject before paying the upload cost if the bindless array is full.
        self.check_slot_available()?;

        let texture = Texture::from_dds(
            device,
            allocator,
            queue,
            command_pool,
            dds_bytes,
            self.shared_sampler,
            self.staging_pool.as_mut(),
        )
        .with_context(|| format!("Failed to load DDS texture '{}'", path))?;

        let handle = self.textures.len() as TextureHandle;
        self.write_texture_to_all_sets(device, handle, &texture);
        self.textures.push(TextureEntry {
            texture: Some(texture),
            pending_destroy: VecDeque::new(),
            ref_count: 1,
        });
        self.path_map.insert(normalized, handle);

        Ok(handle)
    }

    /// Look up a cached texture by path. Returns `None` if not loaded.
    ///
    /// Read-only probe — does **not** bump the entry's refcount. Use
    /// [`acquire_by_path`](Self::acquire_by_path) when the caller
    /// intends to hold the handle and must pair it with a
    /// `drop_texture`. See #524.
    pub fn get_by_path(&self, path: &str) -> Option<TextureHandle> {
        self.path_map.get(&normalize_path(path)).copied()
    }

    /// Acquire a texture handle by path, bumping the refcount on hit.
    ///
    /// Mirror of [`get_by_path`](Self::get_by_path) but with the
    /// refcount side-effect. The caller must pair this with a single
    /// `drop_texture` when the handle is no longer in use. `resolve_texture`
    /// uses this on its fast path (before falling through to
    /// `load_dds` on miss) so every cell-loader resolve ends up with
    /// exactly one acquire. See #524.
    pub fn acquire_by_path(&mut self, path: &str) -> Option<TextureHandle> {
        let normalized = normalize_path(path);
        let &handle = self.path_map.get(&normalized)?;
        let entry = self.textures.get_mut(handle as usize)?;
        entry.ref_count = entry.ref_count.saturating_add(1);
        Some(handle)
    }

    /// Handle for the fallback checkerboard texture (always 0).
    pub fn fallback(&self) -> TextureHandle {
        self.fallback_handle
    }

    /// Get the bindless descriptor set for a frame-in-flight.
    ///
    /// This single set contains ALL textures. Bind it once per frame —
    /// the fragment shader indexes into `textures[texture_index]` via
    /// the per-instance data.
    pub fn descriptor_set(&self, frame_index: usize) -> vk::DescriptorSet {
        self.bindless_sets[frame_index]
    }

    /// Register an RGBA texture directly (for dynamic UI textures).
    pub fn register_rgba(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<TextureHandle> {
        self.check_slot_available()?;

        let texture = Texture::from_rgba(
            device,
            allocator,
            queue,
            command_pool,
            width,
            height,
            pixels,
            self.shared_sampler,
            self.staging_pool.as_mut(),
        )
        .context("Failed to create dynamic RGBA texture")?;

        let handle = self.textures.len() as TextureHandle;
        self.write_texture_to_all_sets(device, handle, &texture);
        self.textures.push(TextureEntry {
            texture: Some(texture),
            pending_destroy: VecDeque::new(),
            ref_count: 1,
        });

        Ok(handle)
    }

    /// Drop a texture. Its GPU resources move into the deferred-destroy
    /// ring, the bindless descriptor slot is redirected to the fallback
    /// checkerboard (so any stale draw call degrades gracefully instead
    /// of sampling a freed `VkImageView`), and the path-cache entry is
    /// purged so a re-upload of the same path produces a fresh handle.
    ///
    /// Handles stay stable: the dropped slot retains its index in the
    /// bindless array forever — reuse would produce silent material
    /// corruption on any dangling `GpuInstance.texture_index` reference.
    /// See #372. No-op on an unknown or already-dropped handle.
    pub fn drop_texture(&mut self, device: &ash::Device, handle: TextureHandle) {
        if !self.release_ref(handle) {
            return;
        }
        // Last ref released — perform the GPU-side drop. `release_ref`
        // already purged `path_map` so subsequent `load_dds` for the
        // same path creates a fresh entry.
        let Some(entry) = self.textures.get_mut(handle as usize) else {
            return;
        };
        let Some(old) = entry.texture.take() else {
            return;
        };
        entry
            .pending_destroy
            .push_back((self.current_frame_id, old));

        // Redirect the bindless slot to the fallback texture so any
        // GpuInstance still referencing this handle reads the
        // checkerboard instead of a freed image view. Current slot is
        // written immediately; other slots queued until their fence
        // signals (#92).
        let fallback_idx = self.fallback_handle as usize;
        if fallback_idx < self.textures.len() {
            if let Some(fallback) = self.textures[fallback_idx].texture.as_ref() {
                let image_view = fallback.image_view;
                let sampler = fallback.sampler;
                self.apply_descriptor_write(device, handle, image_view, sampler);
            }
        }
    }

    /// Decrement the refcount for `handle` and purge the `path_map`
    /// entry when the last holder releases. Returns `true` iff the
    /// caller should proceed with a GPU-side drop (handle was live and
    /// this release took it to zero). The GPU-side work needs an
    /// `ash::Device`, so it's split out into [`drop_texture`](Self::drop_texture);
    /// the Vulkan-free half lives here so tests can exercise refcount
    /// invariants without a real device. See #524.
    fn release_ref(&mut self, handle: TextureHandle) -> bool {
        let Some(entry) = self.textures.get_mut(handle as usize) else {
            return false;
        };
        if entry.ref_count == 0 {
            log::warn!(
                "drop_texture({}) on already-released handle (ref_count was 0)",
                handle,
            );
            return false;
        }
        entry.ref_count -= 1;
        if entry.ref_count > 0 {
            return false;
        }
        // Purge path_map so a subsequent `load_dds` of the same path
        // creates a fresh texture. Linear scan is fine: drops happen
        // on cell unload, not per-frame.
        self.path_map.retain(|_, &mut h| h != handle);
        true
    }

    /// Drain deferred-destroy queues across all entries, destroying
    /// textures whose age is now `>= MAX_FRAMES_IN_FLIGHT`. Called once
    /// per frame alongside the mesh/BLAS deferred-destroy ticks. The
    /// `update_rgba` path also drains inline; this pass catches entries
    /// queued by [`drop_texture`] where no subsequent update call fires.
    pub fn tick_deferred_destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        let current_frame_id = self.current_frame_id;
        for entry in &mut self.textures {
            while let Some(&(queued, _)) = entry.pending_destroy.front() {
                if !should_destroy_pending(current_frame_id, queued) {
                    break;
                }
                if let Some((_, mut old)) = entry.pending_destroy.pop_front() {
                    old.destroy(device, allocator);
                }
            }
        }
    }

    /// Advance the frame counter for deferred-destroy aging (issue #134).
    /// Advance the deferred-destroy frame counter and mark `slot` as
    /// the descriptor-set that will be recorded next. The caller MUST
    /// have already waited on `slot`'s fence before entering this
    /// function — that's what makes the pending-descriptor-write flush
    /// below safe (no command buffer can still be referencing
    /// `bindless_sets[slot]` at this point). See #92.
    pub fn begin_frame(&mut self, device: &ash::Device, slot: usize) {
        self.current_frame_id = self.current_frame_id.wrapping_add(1);
        self.current_slot = slot;
        self.flush_pending_set_writes(device, slot);
    }

    /// Apply every queued descriptor write for `slot` via a single
    /// `vkUpdateDescriptorSets` batch. Safe to call only when the
    /// caller has waited on `slot`'s fence.
    fn flush_pending_set_writes(&mut self, device: &ash::Device, slot: usize) {
        if self.pending_set_writes[slot].is_empty() {
            return;
        }
        // Snapshot the queue so we can drain without holding a borrow
        // on `self.pending_set_writes` while we read `self.bindless_sets`.
        let drained: Vec<PendingSetWrite> = std::mem::take(&mut self.pending_set_writes[slot]);
        let set = self.bindless_sets[slot];
        // `DescriptorImageInfo` must outlive the `WriteDescriptorSet`
        // referencing it, so we build parallel vectors and index in
        // lockstep.
        let image_infos: Vec<vk::DescriptorImageInfo> = drained
            .iter()
            .map(|w| {
                vk::DescriptorImageInfo::default()
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .image_view(w.image_view)
                    .sampler(w.sampler)
            })
            .collect();
        let writes: Vec<vk::WriteDescriptorSet> = drained
            .iter()
            .enumerate()
            .map(|(i, w)| {
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(0)
                    .dst_array_element(w.handle)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(std::slice::from_ref(&image_infos[i]))
            })
            .collect();
        unsafe {
            device.update_descriptor_sets(&writes, &[]);
        }
    }

    /// Apply a descriptor write by writing to the current recording
    /// slot immediately and queuing the same payload for every other
    /// slot. The deferred writes fire from `begin_frame` on each slot's
    /// next turn, after the caller has waited on that slot's fence.
    /// See #92.
    fn apply_descriptor_write(
        &mut self,
        device: &ash::Device,
        handle: TextureHandle,
        image_view: vk::ImageView,
        sampler: vk::Sampler,
    ) {
        // Immediate write on the current slot — safe: we're CPU-side
        // recording its command buffer right now; no submission is
        // pending against it.
        let image_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(image_view)
            .sampler(sampler);
        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.bindless_sets[self.current_slot])
            .dst_binding(0)
            .dst_array_element(handle)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(std::slice::from_ref(&image_info));
        unsafe {
            device.update_descriptor_sets(&[write], &[]);
        }
        self.record_pending_writes_for_other_slots(handle, image_view, sampler);
    }

    /// Pure queue bookkeeping: push a pending write onto every slot
    /// except `current_slot`. Split out of `apply_descriptor_write`
    /// so unit tests can exercise the queue mechanics without a real
    /// Vulkan device. See #92.
    fn record_pending_writes_for_other_slots(
        &mut self,
        handle: TextureHandle,
        image_view: vk::ImageView,
        sampler: vk::Sampler,
    ) {
        for (slot, queue) in self.pending_set_writes.iter_mut().enumerate() {
            if slot == self.current_slot {
                continue;
            }
            queue.push(PendingSetWrite {
                handle,
                image_view,
                sampler,
            });
        }
    }

    #[cfg(test)]
    pub(crate) fn current_frame_id(&self) -> u64 {
        self.current_frame_id
    }

    /// Replace the texture data for an existing handle with new RGBA pixels.
    ///
    /// Uses deferred destruction: the replaced texture is kept alive until
    /// `MAX_FRAMES_IN_FLIGHT` frames have elapsed. See issue #134.
    pub fn update_rgba(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        handle: TextureHandle,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<()> {
        let current_frame_id = self.current_frame_id;
        let entry = &mut self.textures[handle as usize];

        // Drain old textures that have aged out.
        while let Some(&(queued, _)) = entry.pending_destroy.front() {
            if !should_destroy_pending(current_frame_id, queued) {
                break;
            }
            if let Some((_, mut old)) = entry.pending_destroy.pop_front() {
                old.destroy(device, allocator);
            }
        }

        // Swap in the new texture. If the entry was dropped earlier this
        // quietly revives it (bindless slot reactivates on the descriptor
        // write below).
        let new_texture = Texture::from_rgba(
            device,
            allocator,
            queue,
            command_pool,
            width,
            height,
            pixels,
            self.shared_sampler,
            self.staging_pool.as_mut(),
        )
        .context("Failed to create updated dynamic RGBA texture")?;
        if let Some(prev) = entry.texture.replace(new_texture) {
            entry.pending_destroy.push_back((current_frame_id, prev));
        }

        // Update the bindless array entry. Current slot writes
        // immediately; other slots are queued until their fence
        // signals in `begin_frame` (#92).
        let live = self.textures[handle as usize]
            .texture
            .as_ref()
            .expect("entry was just populated above");
        let image_view = live.image_view;
        let sampler = live.sampler;
        self.apply_descriptor_write(device, handle, image_view, sampler);

        Ok(())
    }

    /// Number of loaded textures (including fallback).
    pub fn len(&self) -> usize {
        self.textures.len()
    }

    /// Recreate descriptor sets for a new swapchain.
    ///
    /// With bindless textures, the descriptor sets are independent of swapchain
    /// image count. This method recreates them to ensure a clean state and
    /// re-writes all texture bindings.
    pub fn recreate_descriptor_sets(
        &mut self,
        device: &ash::Device,
        _new_swapchain_image_count: u32,
    ) -> Result<()> {
        // Destroy old pool (frees all sets implicitly).
        unsafe {
            device.destroy_descriptor_pool(self.descriptor_pool, None);
        }

        // Recreate pool + sets (must match new() flags: UPDATE_AFTER_BIND).
        let pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: self.max_textures * MAX_FRAMES_IN_FLIGHT as u32,
        };
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND)
            .pool_sizes(std::slice::from_ref(&pool_size))
            .max_sets(MAX_FRAMES_IN_FLIGHT as u32);
        self.descriptor_pool = unsafe {
            device
                .create_descriptor_pool(&pool_info, None)
                .context("Failed to recreate bindless texture descriptor pool")?
        };

        let layouts = vec![self.descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.descriptor_pool)
            .set_layouts(&layouts);
        self.bindless_sets = unsafe {
            device
                .allocate_descriptor_sets(&alloc_info)
                .context("Failed to reallocate bindless texture descriptor sets")?
        };

        // Fresh sets have no stale pending writes to replay — the
        // per-slot queue is invalid against the new `VkDescriptorSet`
        // handles so discard it. The re-write loop below writes every
        // live texture directly. See #92.
        for queue in &mut self.pending_set_writes {
            queue.clear();
        }

        // Re-write all texture bindings. Skip dropped slots — the new
        // descriptor set starts fresh, and the loop in drop_texture
        // will redirect them to the fallback on their next update.
        // Collect into a Vec first so the `self.textures` immutable
        // borrow doesn't alias `apply_descriptor_write`'s `&mut self`.
        let rewrites: Vec<(TextureHandle, vk::ImageView, vk::Sampler)> = self
            .textures
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| {
                entry
                    .texture
                    .as_ref()
                    .map(|t| (i as TextureHandle, t.image_view, t.sampler))
            })
            .collect();
        for (handle, image_view, sampler) in rewrites {
            self.apply_descriptor_write(device, handle, image_view, sampler);
        }

        log::info!(
            "TextureRegistry descriptor sets recreated: {} textures (bindless)",
            self.textures.len(),
        );

        Ok(())
    }

    /// Destroy all textures, descriptor pool, and layout.
    pub fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for entry in &mut self.textures {
            for (_, mut pending) in entry.pending_destroy.drain(..) {
                pending.destroy(device, allocator);
            }
            if let Some(mut t) = entry.texture.take() {
                t.destroy(device, allocator);
            }
        }
        self.textures.clear();
        self.path_map.clear();

        // Tear down the texture staging pool before the descriptor
        // set + sampler — the pool holds VkBuffers that must be
        // destroyed while the device is still valid. See #239.
        if let Some(pool) = self.staging_pool.as_mut() {
            pool.destroy();
        }

        unsafe {
            device.destroy_sampler(self.shared_sampler, None);
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
        }
    }

    /// Write a texture's image view + sampler to the bindless array.
    ///
    /// Writes the current recording slot immediately and queues the
    /// same payload on the remaining slots, which flush in their next
    /// `begin_frame` after the caller waits on that slot's fence. See
    /// `apply_descriptor_write` + #92.
    fn write_texture_to_all_sets(
        &mut self,
        device: &ash::Device,
        handle: TextureHandle,
        texture: &Texture,
    ) {
        self.write_texture_to_sets_inner(device, handle, texture);
    }

    fn write_texture_to_sets_inner(
        &mut self,
        device: &ash::Device,
        handle: TextureHandle,
        texture: &Texture,
    ) {
        self.apply_descriptor_write(device, handle, texture.image_view, texture.sampler);
    }
}

/// Normalize a texture path for use as a `path_map` cache key.
///
/// Canonical form: lowercase, forward slashes, guaranteed `textures/`
/// prefix. Matches the canonicalization
/// `asset_provider::normalize_texture_path` applies for BSA/BA2
/// lookups, so both sites — the archive extract and the
/// `TextureRegistry` cache — agree on what the "same texture" means.
///
/// Pre-#522 this stopped at lowercase + slash-flip. The `textures\`
/// prefix was added opportunistically by `asset_provider::extract` for
/// the archive lookup but never propagated back into the registry's
/// cache key, so two callers passing `landscape\dirt02.dds` and
/// `textures\landscape\dirt02.dds` for the same texel produced
/// separate cache entries + separate bindless slots + double the
/// VRAM. Silent — no log, no error. See #522.
fn normalize_path(path: &str) -> String {
    let lowered = path.to_ascii_lowercase().replace('\\', "/");
    if lowered.starts_with("textures/") {
        lowered
    } else {
        format!("textures/{}", lowered)
    }
}

fn should_destroy_pending(current_frame: u64, queued_frame: u64) -> bool {
    current_frame.wrapping_sub(queued_frame) >= MAX_FRAMES_IN_FLIGHT as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_backslashes_and_case() {
        assert_eq!(
            normalize_path(r"Textures\Architecture\Walls\Wall01_d.dds"),
            "textures/architecture/walls/wall01_d.dds"
        );
    }

    #[test]
    fn normalize_already_clean() {
        assert_eq!(
            normalize_path("textures/clutter/food/beerbottle.dds"),
            "textures/clutter/food/beerbottle.dds"
        );
    }

    /// Regression for #522: prefixed and unprefixed inputs MUST
    /// canonicalize to the same cache key. Pre-fix, `landscape/dirt02.dds`
    /// and `textures\landscape\dirt02.dds` mapped to different keys,
    /// producing silent bindless slot duplication on terrain tiles
    /// whose LTEX record omits the prefix (FO3/FNV vanilla) while
    /// `spawn_terrain_mesh` at cell_loader.rs:945 re-calls with the
    /// fully-qualified path. Matches the canonicalization that
    /// `asset_provider::normalize_texture_path` applies for the
    /// archive lookup.
    #[test]
    fn normalize_prefix_variants_collapse_to_one_key() {
        let unprefixed = normalize_path(r"landscape\dirt02.dds");
        let prefixed = normalize_path(r"textures\landscape\dirt02.dds");
        let mixed_slashes = normalize_path("Textures/LANDSCAPE/dirt02.DDS");
        let forward_only = normalize_path("landscape/dirt02.dds");

        assert_eq!(unprefixed, "textures/landscape/dirt02.dds");
        assert_eq!(prefixed, "textures/landscape/dirt02.dds");
        assert_eq!(mixed_slashes, "textures/landscape/dirt02.dds");
        assert_eq!(forward_only, "textures/landscape/dirt02.dds");
    }

    /// Edge case: a path that happens to start with something LIKE
    /// "textures" but isn't the directory prefix must still get the
    /// prefix added. `texturesets/foo.dds` is not a textures-rooted
    /// path — it would be rooted at `textures/texturesets/…` on disk.
    #[test]
    fn normalize_similar_prefix_is_not_swallowed() {
        let similar = normalize_path("texturesets/foo.dds");
        assert_eq!(similar, "textures/texturesets/foo.dds");
    }

    #[test]
    fn should_destroy_pending_honors_frame_gap() {
        assert!(!should_destroy_pending(0, 0));
        assert!(!should_destroy_pending(1, 0));
        assert!(should_destroy_pending(MAX_FRAMES_IN_FLIGHT as u64, 0));
        assert!(should_destroy_pending(1000, 0));
    }

    #[test]
    fn multiple_same_frame_calls_do_not_authorize_destruction() {
        let current_frame = 10;
        for _ in 0..5 {
            assert!(!should_destroy_pending(current_frame, current_frame));
        }
        assert!(should_destroy_pending(
            current_frame + MAX_FRAMES_IN_FLIGHT as u64,
            current_frame
        ));
    }

    #[test]
    fn frame_counter_math_is_wrap_safe() {
        let queued = u64::MAX - 1;
        let current = queued.wrapping_add(MAX_FRAMES_IN_FLIGHT as u64);
        assert!(should_destroy_pending(current, queued));
        let current = queued.wrapping_add(MAX_FRAMES_IN_FLIGHT as u64 - 1);
        assert!(!should_destroy_pending(current, queued));
    }

    /// Build a registry in a test-only state: `check_slot_available` only
    /// reads `textures` + `max_textures`, so we forge a partial
    /// `TextureRegistry` without touching Vulkan.
    fn make_registry_for_overflow_test(max_textures: u32, occupied: usize) -> TextureRegistry {
        TextureRegistry {
            textures: (0..occupied)
                .map(|_| TextureEntry {
                    texture: None,
                    pending_destroy: VecDeque::new(),
                    ref_count: 0,
                })
                .collect(),
            path_map: HashMap::new(),
            fallback_handle: 0,
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            bindless_sets: Vec::new(),
            shared_sampler: vk::Sampler::null(),
            max_textures,
            current_frame_id: 0,
            // Unit-test path: `check_slot_available` doesn't touch the
            // pool, so None is safe here.
            staging_pool: None,
            pending_set_writes: vec![Vec::new(); MAX_FRAMES_IN_FLIGHT],
            current_slot: 0,
        }
    }

    #[test]
    fn slot_available_when_under_bound() {
        let reg = make_registry_for_overflow_test(1024, 512);
        reg.check_slot_available()
            .expect("half-full registry should accept new textures");
    }

    #[test]
    fn slot_rejected_at_exact_bound() {
        // Regression for #425 — old code silently wrote past the bindless
        // array bound once textures.len() == max_textures.
        let reg = make_registry_for_overflow_test(1024, 1024);
        let err = reg
            .check_slot_available()
            .expect_err("full registry must refuse new textures");
        let msg = format!("{err}");
        assert!(
            msg.contains("1024 of 1024"),
            "message reports counts: {msg}"
        );
        assert!(msg.contains("#425"), "message references the issue: {msg}");
    }

    #[test]
    fn slot_rejected_beyond_bound() {
        let reg = make_registry_for_overflow_test(16, 16);
        assert!(reg.check_slot_available().is_err());
    }

    /// Seed a registry with the fallback at handle 0 and one
    /// path-mapped entry at handle 1 carrying `initial_ref_count`.
    /// Both entries have `texture: None` so the pure-Rust bits of
    /// `drop_texture` run without calling into Vulkan.
    fn make_registry_with_entry(path: &str, initial_ref_count: u32) -> TextureRegistry {
        let mut reg = make_registry_for_overflow_test(16, 0);
        reg.textures.push(TextureEntry {
            texture: None,
            pending_destroy: VecDeque::new(),
            ref_count: u32::MAX,
        });
        reg.fallback_handle = 0;
        reg.textures.push(TextureEntry {
            texture: None,
            pending_destroy: VecDeque::new(),
            ref_count: initial_ref_count,
        });
        reg.path_map.insert(normalize_path(path), 1);
        reg
    }

    #[test]
    fn acquire_by_path_bumps_refcount() {
        // #524 — a second resolve of the same path must acquire a ref,
        // otherwise cell A's unload would free the texture that cell B
        // is still relying on.
        let mut reg = make_registry_with_entry("chair.dds", 1);
        let h1 = reg.acquire_by_path("chair.dds");
        assert_eq!(h1, Some(1));
        assert_eq!(reg.textures[1].ref_count, 2);
        let h2 = reg.acquire_by_path("chair.dds");
        assert_eq!(h2, Some(1));
        assert_eq!(reg.textures[1].ref_count, 3);
    }

    #[test]
    fn acquire_by_path_miss_returns_none() {
        let mut reg = make_registry_with_entry("chair.dds", 1);
        assert_eq!(reg.acquire_by_path("barrel.dds"), None);
        assert_eq!(
            reg.textures[1].ref_count, 1,
            "missed lookups must not touch unrelated entries"
        );
    }

    #[test]
    fn get_by_path_does_not_bump() {
        // Read-only probe — debug/inspect commands rely on this.
        let reg = make_registry_with_entry("chair.dds", 1);
        assert_eq!(reg.get_by_path("chair.dds"), Some(1));
        assert_eq!(reg.textures[1].ref_count, 1);
    }

    #[test]
    fn release_ref_decrements_without_freeing_until_zero() {
        // Cell A + cell B both hold a ref. Cell A unloads: decrement to
        // 1, texture entry stays live. Cell B unloads: decrement to 0,
        // path_map purged so a subsequent load creates a fresh entry.
        let mut reg = make_registry_with_entry("chair.dds", 2);
        assert!(
            !reg.release_ref(1),
            "first release should not authorise a GPU drop"
        );
        assert_eq!(reg.textures[1].ref_count, 1);
        assert!(
            reg.path_map.contains_key("textures/chair.dds"),
            "cell B still holds a ref — path_map must survive"
        );
        assert!(
            reg.release_ref(1),
            "last release must authorise the GPU drop"
        );
        assert_eq!(reg.textures[1].ref_count, 0);
        assert!(
            !reg.path_map.contains_key("textures/chair.dds"),
            "last release purges path_map"
        );
    }

    #[test]
    fn release_ref_on_zero_refcount_warns_and_bails() {
        // Double-free guard: returns false without underflowing.
        let mut reg = make_registry_with_entry("chair.dds", 0);
        assert!(!reg.release_ref(1));
        assert_eq!(reg.textures[1].ref_count, 0);
    }

    #[test]
    fn release_ref_on_unknown_handle_is_noop() {
        let mut reg = make_registry_with_entry("chair.dds", 1);
        assert!(!reg.release_ref(99));
        assert_eq!(
            reg.textures[1].ref_count, 1,
            "unrelated handles must not be touched"
        );
    }

    #[test]
    fn fallback_refcount_sticky() {
        // Fallback handle is process-wide and must never underflow
        // from stray drops. `u32::MAX` gives plenty of headroom.
        let reg = make_registry_with_entry("chair.dds", 1);
        assert_eq!(reg.textures[0].ref_count, u32::MAX);
    }

    // ── #92 pending descriptor-write queue mechanics ────────────────

    /// Regression for #92 — a descriptor update against the current
    /// recording slot must NOT be pushed to the pending queue (the
    /// current slot is written immediately); every OTHER slot must
    /// receive a queued write so the caller can flush it safely after
    /// that slot's fence signals.
    #[test]
    fn pending_write_records_on_other_slots_only() {
        let mut reg = make_registry_for_overflow_test(16, 0);
        reg.current_slot = 0;
        let image_view = vk::ImageView::null();
        let sampler = vk::Sampler::null();

        reg.record_pending_writes_for_other_slots(7, image_view, sampler);

        // Current slot (0): empty.
        assert_eq!(reg.pending_set_writes[0].len(), 0);
        // Other slot (1): received the deferred write.
        assert_eq!(reg.pending_set_writes[1].len(), 1);
        assert_eq!(reg.pending_set_writes[1][0].handle, 7);
    }

    /// Swapping the current slot flips which queue receives deferred
    /// writes — the one previously "current" now accumulates pending
    /// updates, matching the guarantee `begin_frame` relies on when
    /// the caller ticks to a new slot.
    #[test]
    fn pending_write_current_slot_change_flips_deferred_target() {
        let mut reg = make_registry_for_overflow_test(16, 0);
        reg.current_slot = 0;
        reg.record_pending_writes_for_other_slots(1, vk::ImageView::null(), vk::Sampler::null());
        assert_eq!(reg.pending_set_writes[0].len(), 0);
        assert_eq!(reg.pending_set_writes[1].len(), 1);

        // Flip to slot 1 as the new current slot (begin_frame would
        // do this after the caller waits on slot 1's fence). A
        // subsequent write now queues on slot 0 instead.
        reg.current_slot = 1;
        reg.record_pending_writes_for_other_slots(2, vk::ImageView::null(), vk::Sampler::null());
        assert_eq!(reg.pending_set_writes[0].len(), 1);
        assert_eq!(reg.pending_set_writes[0][0].handle, 2);
        // Slot 1's queue is untouched by this call (handle 2 didn't
        // land there) — it still holds the original handle 1 from
        // before the flip.
        assert_eq!(reg.pending_set_writes[1].len(), 1);
        assert_eq!(reg.pending_set_writes[1][0].handle, 1);
    }

    /// Multiple writes accumulate in deferred slots — each one must
    /// be replayed on flush, in authoring order. Guards against a
    /// "last-write-wins" regression.
    #[test]
    fn pending_writes_accumulate_and_preserve_order() {
        let mut reg = make_registry_for_overflow_test(16, 0);
        reg.current_slot = 0;
        for handle in [3, 7, 11, 4] {
            reg.record_pending_writes_for_other_slots(
                handle,
                vk::ImageView::null(),
                vk::Sampler::null(),
            );
        }
        let deferred = &reg.pending_set_writes[1];
        assert_eq!(deferred.len(), 4);
        assert_eq!(
            deferred.iter().map(|w| w.handle).collect::<Vec<_>>(),
            vec![3, 7, 11, 4],
        );
    }

    /// `recreate_descriptor_sets` allocates fresh `VkDescriptorSet`
    /// handles, so every pending write queued against the old sets
    /// is invalid. Verify the queue is cleared as part of the
    /// recreate-path contract (#92 — stale handles must not flow
    /// into a fresh set in `flush_pending_set_writes`).
    #[test]
    fn pending_writes_cleared_by_recreate_semantics() {
        let mut reg = make_registry_for_overflow_test(16, 0);
        reg.current_slot = 0;
        reg.record_pending_writes_for_other_slots(5, vk::ImageView::null(), vk::Sampler::null());
        assert!(!reg.pending_set_writes[1].is_empty());

        // Simulate the `recreate_descriptor_sets` queue-clear step
        // directly (the Vulkan side needs a real device, out of
        // scope here).
        for queue in &mut reg.pending_set_writes {
            queue.clear();
        }
        assert!(reg.pending_set_writes.iter().all(|q| q.is_empty()));
    }
}
