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

/// One queued DDS upload waiting for the next batched
/// `flush_pending_uploads` call. Pre-#881 every fresh DDS paid its
/// own `with_one_time_commands` (submit + fence-wait); the queue
/// collects them so a cell-load completion gate can drain N uploads
/// with ONE submit + ONE fence-wait. The DDS bytes are owned (boxed
/// `Vec<u8>`) because the parser holds borrowed slices and we don't
/// retain the source `BsaArchive` extraction past `acquire_by_path`'s
/// return. See `flush_pending_uploads` for the drain side.
struct PendingDdsUpload {
    handle: TextureHandle,
    dds_bytes: Vec<u8>,
    clamp_mode: u8,
    /// Lowercased path — for diagnostic logging only. The path_map
    /// entry is set up by `enqueue_dds_with_clamp` at queue time.
    path: String,
}

/// Outcome of [`TextureRegistry::queue_or_hit`]. Distinguishes a
/// fresh reservation (caller must redirect the descriptor to the
/// fallback) from a cache hit (descriptor already wired). Both
/// variants carry the resulting bindless handle.
#[derive(Debug, Clone, Copy)]
enum EnqueueOutcome {
    Reserved(TextureHandle),
    Hit(TextureHandle),
}

impl EnqueueOutcome {
    fn handle(&self) -> TextureHandle {
        match self {
            EnqueueOutcome::Reserved(h) | EnqueueOutcome::Hit(h) => *h,
        }
    }
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
    /// Default LINEAR/REPEAT sampler. Aliased onto `samplers[0]` so
    /// the legacy `shared_sampler` field stays valid for non-bindless
    /// callers (composite.rs reads it for the HDR sampler in some
    /// codepaths).
    pub shared_sampler: vk::Sampler,
    /// Samplers indexed by Gamebryo `TexClampMode` (`#610`). Encoding
    /// matches nif.xml's `<enum name="TexClampMode">`:
    ///   0 = CLAMP_S_CLAMP_T  (decals / scope reticles / skybox seams)
    ///   1 = CLAMP_S_WRAP_T
    ///   2 = WRAP_S_CLAMP_T
    ///   3 = WRAP_S_WRAP_T    (default — the legacy REPEAT/REPEAT)
    /// Decoded from the lower 4 bits of `TexDesc.flags` in the parser
    /// (`crates/nif/src/blocks/properties.rs:464`) and from
    /// `BSEffectShaderProperty.texture_clamp_mode` directly. Pre-#610
    /// the renderer hardcoded REPEAT for every texture and CLAMP-
    /// authored decals / Oblivion architecture trim rendered with
    /// bleeding edges.
    samplers: [vk::Sampler; 4],
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
    /// DDS uploads queued by `enqueue_dds_with_clamp` and drained by
    /// `flush_pending_uploads` (#881 / CELL-PERF-03). Each entry's
    /// bindless slot is already reserved (with the descriptor
    /// redirected to the fallback texture) so callers can attach the
    /// returned `TextureHandle` to a placement immediately; the
    /// real image upload — and the descriptor write that points the
    /// slot at it — happens during the batched flush. Mirrors the
    /// `pending_set_writes` deferred-flush pattern (#92), extended
    /// from descriptor writes to the upload itself.
    pending_dds_uploads: Vec<PendingDdsUpload>,
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

        // Build one sampler per Gamebryo `TexClampMode` value (4
        // total) so a per-texture `clamp_mode` can route to the right
        // VkSamplerAddressMode pair. All four share the LINEAR /
        // anisotropic / mipmap-LINEAR filtering setup; only U/V wrap
        // axes differ. See #610 / D4-NEW-02.
        let anisotropy_enable = max_sampler_anisotropy > 1.0;
        let max_anisotropy = if anisotropy_enable {
            max_sampler_anisotropy
        } else {
            1.0
        };
        let make_sampler = |u_mode: vk::SamplerAddressMode,
                            v_mode: vk::SamplerAddressMode|
         -> Result<vk::Sampler> {
            let info = vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(u_mode)
                .address_mode_v(v_mode)
                // W axis is unused on 2D bindless reads — set to REPEAT
                // for consistency with the pre-#610 single-sampler shape.
                .address_mode_w(vk::SamplerAddressMode::REPEAT)
                .anisotropy_enable(anisotropy_enable)
                .max_anisotropy(max_anisotropy)
                .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
                .unnormalized_coordinates(false)
                .compare_enable(false)
                .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                .min_lod(0.0)
                .max_lod(16.0);
            unsafe {
                device
                    .create_sampler(&info, None)
                    .context("Failed to create texture sampler")
            }
        };
        // Index order matches `TexClampMode` from nif.xml. The lower 4
        // bits of `TexDesc.flags` are written in this exact encoding by
        // the parser at `properties.rs:464`, and BSEffectShaderProperty
        // ships the same encoding directly. Note `0` is CLAMP/CLAMP
        // (the audit's primary fix target — decals / scope reticles)
        // and `3` is REPEAT/REPEAT (the default Skyrim wrap).
        let samplers = [
            // 0 = CLAMP_S_CLAMP_T — full clamp: decals, scope reticles,
            // skybox seams, the audit's primary fix target.
            make_sampler(
                vk::SamplerAddressMode::CLAMP_TO_EDGE,
                vk::SamplerAddressMode::CLAMP_TO_EDGE,
            )?,
            // 1 = CLAMP_S_WRAP_T — vertical strips that should clamp
            // horizontally and repeat vertically.
            make_sampler(
                vk::SamplerAddressMode::CLAMP_TO_EDGE,
                vk::SamplerAddressMode::REPEAT,
            )?,
            // 2 = WRAP_S_CLAMP_T — mirror of mode 1 (cylindrical labels
            // etc.).
            make_sampler(
                vk::SamplerAddressMode::REPEAT,
                vk::SamplerAddressMode::CLAMP_TO_EDGE,
            )?,
            // 3 = WRAP_S_WRAP_T — pre-#610 default for everything.
            make_sampler(vk::SamplerAddressMode::REPEAT, vk::SamplerAddressMode::REPEAT)?,
        ];
        // Legacy public field — kept as an alias on `samplers[3]` (the
        // REPEAT/REPEAT entry) so any non-bindless caller that reads
        // it (composite.rs's HDR sampler path) keeps the pre-#610
        // wrap behaviour.
        let shared_sampler = samplers[3];

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
            samplers,
            max_textures,
            current_frame_id: 0,
            staging_pool,
            pending_set_writes: vec![Vec::new(); MAX_FRAMES_IN_FLIGHT],
            current_slot: 0,
            pending_dds_uploads: Vec::new(),
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
    /// See #524. Uses the default WRAP_S_WRAP_T (REPEAT/REPEAT) sampler
    /// — call [`Self::load_dds_with_clamp`] with the source material's
    /// `TexClampMode` (`0..=3` — see `samplers`) when it differs, so
    /// the descriptor write picks the matching
    /// `VkSamplerAddressMode` pair. The default arm also covers
    /// content that has no authored clamp data (procedural / engine-
    /// fallback textures). See #610.
    pub fn load_dds(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        path: &str,
        dds_bytes: &[u8],
    ) -> Result<TextureHandle> {
        // 3 = WRAP_S_WRAP_T per nif.xml — the legacy REPEAT/REPEAT.
        self.load_dds_with_clamp(device, allocator, queue, command_pool, path, dds_bytes, 3)
    }

    /// Load a DDS texture with an explicit Gamebryo `TexClampMode`
    /// (`0..=3`, see `samplers` field). Same caching + refcount
    /// semantics as [`Self::load_dds`]; the only behavioural difference
    /// is the sampler bound to the bindless descriptor entry.
    ///
    /// `clamp_mode` values outside `0..=3` are clamped to `0` (REPEAT)
    /// — defensive default for upstream parser garbage.
    ///
    /// Cache key includes `clamp_mode`: the same `path` requested with
    /// two different clamp modes produces two distinct entries (the
    /// underlying GPU image is uploaded twice — acceptable since this
    /// is rare in Bethesda content; per-material clamp_mode is the
    /// almost-universal authoring pattern). See #610 / D4-NEW-02.
    pub fn load_dds_with_clamp(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        path: &str,
        dds_bytes: &[u8],
        clamp_mode: u8,
    ) -> Result<TextureHandle> {
        let clamp_mode = clamp_mode.min(3);
        let normalized = clamp_keyed_path(path, clamp_mode);

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
            self.samplers[clamp_mode as usize],
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

    /// Enqueue a DDS upload for batched flush. Counterpart of
    /// [`Self::load_dds_with_clamp`]: same cache hit semantics
    /// (returns the existing handle with refcount bumped on hit) but
    /// the miss path defers the GPU upload + descriptor write until
    /// the next [`Self::flush_pending_uploads`] call. The bindless
    /// slot is reserved eagerly with `texture: None` and the
    /// descriptor redirected to the fallback so any draw issued
    /// before the flush degrades gracefully (sees the checkerboard,
    /// not garbage). See #881 / CELL-PERF-03.
    ///
    /// `dds_bytes` is moved into the queue — the caller must not
    /// retain a reference. Cell-load callers route through
    /// `asset_provider::resolve_texture_with_clamp` which already
    /// owns the bytes via `tex_provider.extract(path)`.
    pub fn enqueue_dds_with_clamp(
        &mut self,
        device: &ash::Device,
        path: &str,
        dds_bytes: Vec<u8>,
        clamp_mode: u8,
    ) -> Result<TextureHandle> {
        let outcome = self.queue_or_hit(path, dds_bytes, clamp_mode)?;
        // For a fresh enqueue, redirect the freshly-reserved
        // descriptor to the fallback checkerboard. Any
        // GpuInstance.texture_index that resolves to this handle
        // before the flush samples the checkerboard instead of an
        // unbound descriptor — same defence `drop_texture` uses on
        // the release side. Cache hits skip this step (the existing
        // entry already has its real descriptor wired).
        if matches!(outcome, EnqueueOutcome::Reserved(_)) {
            let fallback_idx = self.fallback_handle as usize;
            if fallback_idx < self.textures.len() {
                if let Some(fallback) = self.textures[fallback_idx].texture.as_ref() {
                    let image_view = fallback.image_view;
                    let sampler = fallback.sampler;
                    let handle = outcome.handle();
                    self.apply_descriptor_write(device, handle, image_view, sampler);
                }
            }
        }
        Ok(outcome.handle())
    }

    /// Pure-Rust queueing core — slot reservation, refcount bumping,
    /// path_map maintenance, and queue insertion. Split out of
    /// [`Self::enqueue_dds_with_clamp`] so the unit tests can
    /// exercise the bookkeeping (cache miss reserves a fresh slot
    /// vs. cache hit bumps an existing refcount, queue length
    /// transitions, path_map membership) without needing an
    /// `ash::Device` for the fallback descriptor redirect. See
    /// `enqueue_*` tests in the `tests` module below.
    fn queue_or_hit(
        &mut self,
        path: &str,
        dds_bytes: Vec<u8>,
        clamp_mode: u8,
    ) -> Result<EnqueueOutcome> {
        let clamp_mode = clamp_mode.min(3);
        let normalized = clamp_keyed_path(path, clamp_mode);

        // Cache hit: same shape as `load_dds_with_clamp` — bump
        // refcount and return the existing handle without touching
        // the queue.
        if let Some(&handle) = self.path_map.get(&normalized) {
            if let Some(entry) = self.textures.get_mut(handle as usize) {
                entry.ref_count = entry.ref_count.saturating_add(1);
            }
            return Ok(EnqueueOutcome::Hit(handle));
        }

        // Reject before paying the queueing cost if the bindless
        // array is full.
        self.check_slot_available()?;

        // Reserve the bindless slot eagerly. `texture: None` until
        // the flush populates it; refcount = 1 mirrors the immediate
        // upload path so a single `drop_texture` symmetrically
        // releases.
        let handle = self.textures.len() as TextureHandle;
        self.textures.push(TextureEntry {
            texture: None,
            pending_destroy: VecDeque::new(),
            ref_count: 1,
        });
        self.path_map.insert(normalized, handle);
        self.pending_dds_uploads.push(PendingDdsUpload {
            handle,
            dds_bytes,
            clamp_mode,
            path: path.to_string(),
        });
        Ok(EnqueueOutcome::Reserved(handle))
    }

    /// Number of DDS uploads currently queued. Surfaced for the
    /// regression test (#881) and for telemetry / debug commands —
    /// non-zero between cell-load enqueue calls and the matching
    /// `flush_pending_uploads`.
    pub fn pending_dds_upload_count(&self) -> usize {
        self.pending_dds_uploads.len()
    }

    /// Drain the queued DDS uploads with ONE batched submit + ONE
    /// fence-wait. Pre-#881 each `Texture::from_dds_with_mip_chain`
    /// paid its own `vkQueueSubmit` + `vkWaitForFences(.., u64::MAX)`,
    /// so a worldspace edge crossing with 100 fresh DDS textures
    /// burned ~100 sync stalls (~50–100 ms) on the main thread. The
    /// queueing path collapses those to one stall covering all
    /// queued uploads.
    ///
    /// Returns the number of textures uploaded (≥ 0). On any
    /// recording error the queue is left intact so a retry is
    /// possible; the partial command buffer is freed without submit.
    /// On submit/fence error the staging buffers leak into the pool
    /// (the GPU may still be reading them) — a future-proof
    /// alternative would defer-destroy them, but cell-load failure is
    /// already a fatal-style error path.
    ///
    /// Empty queue → no-op (returns `Ok(0)` without allocating a
    /// command buffer or touching the queue mutex).
    pub fn flush_pending_uploads(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        transfer_fence: &std::sync::Mutex<vk::Fence>,
    ) -> Result<usize> {
        if self.pending_dds_uploads.is_empty() {
            return Ok(0);
        }

        // Move the queue out so we can borrow `&mut self` across the
        // record loop without aliasing the field. Any pending entries
        // not flushed (recording error mid-loop) are pushed back at
        // the end so a retry sees a non-empty queue.
        let pending = std::mem::take(&mut self.pending_dds_uploads);
        let count = pending.len();

        // Per-upload outputs assembled during recording; consumed
        // after the submit + wait completes.
        struct StagedUpload {
            handle: TextureHandle,
            texture: super::vulkan::texture::Texture,
            staging: super::vulkan::buffer::StagingGuard,
            staging_capacity: vk::DeviceSize,
        }
        let mut staged: Vec<StagedUpload> = Vec::with_capacity(count);

        // Use the persistent transfer fence (#302) so we don't pay a
        // per-flush vk::Fence create/destroy. `with_one_time_commands_reuse_fence`
        // resets + locks the fence for the duration.
        let record_result = super::vulkan::texture::with_one_time_commands_reuse_fence(
            device,
            queue,
            command_pool,
            transfer_fence,
            |cmd| {
                for upload in &pending {
                    let meta = match super::vulkan::dds::parse_dds(&upload.dds_bytes) {
                        Ok(m) => m,
                        Err(e) => {
                            log::warn!(
                                "Failed to parse DDS '{}': {} — dropping queued upload",
                                upload.path,
                                e,
                            );
                            continue;
                        }
                    };
                    let pixel_data = &upload.dds_bytes[meta.data_offset..];
                    let sampler = self.samplers[upload.clamp_mode as usize];
                    let (texture, staging, staging_capacity) =
                        match super::vulkan::texture::Texture::record_dds_upload(
                            device,
                            allocator,
                            cmd,
                            &meta,
                            pixel_data,
                            sampler,
                            self.staging_pool.as_mut(),
                        ) {
                            Ok(t) => t,
                            Err(e) => {
                                log::warn!(
                                    "Failed to record DDS upload '{}': {} — dropping queued upload",
                                    upload.path,
                                    e,
                                );
                                continue;
                            }
                        };
                    staged.push(StagedUpload {
                        handle: upload.handle,
                        texture,
                        staging,
                        staging_capacity,
                    });
                }
                Ok(())
            },
        );

        // Submit + wait done by `with_one_time_commands_reuse_fence`.
        // After it returns successfully, every recorded upload's GPU
        // work has retired, so each StagingGuard can be released +
        // each Texture installed into its slot's descriptor.
        if let Err(e) = record_result {
            // Recording failure path: nothing was submitted. Best-
            // effort destroy of any partially-staged textures so
            // their VkImage / staging buffer don't leak. The pending
            // queue is gone (we `take`d it); future enqueues will
            // re-populate.
            log::warn!(
                "flush_pending_uploads recording failed ({} uploads dropped): {}",
                staged.len(),
                e,
            );
            for mut s in staged {
                s.texture.destroy(device, allocator);
                s.staging.destroy();
            }
            return Err(e);
        }

        // Install textures + write real descriptors. Replaces the
        // fallback redirect installed at enqueue time.
        let staged_count = staged.len();
        for s in staged {
            let StagedUpload {
                handle,
                texture,
                staging,
                staging_capacity,
            } = s;

            // Write descriptor for the real image view + sampler.
            let image_view = texture.image_view;
            let sampler = texture.sampler;
            self.apply_descriptor_write(device, handle, image_view, sampler);

            // Move the texture into its reserved slot.
            if let Some(entry) = self.textures.get_mut(handle as usize) {
                entry.texture = Some(texture);
            } else {
                log::warn!(
                    "flush_pending_uploads: handle {handle} out of bounds; dropping texture"
                );
            }

            // Release staging back to the pool (or destroy if no
            // pool). The fence-wait above guarantees the GPU is done.
            if let Some(pool) = self.staging_pool.as_mut() {
                staging.release_to(pool, staging_capacity);
            } else {
                staging.destroy();
            }
        }

        log::info!(
            "Flushed {} queued DDS uploads ({} originally enqueued)",
            staged_count,
            count,
        );
        Ok(staged_count)
    }

    /// Look up a cached texture by path. Returns `None` if not loaded.
    ///
    /// Read-only probe — does **not** bump the entry's refcount. Use
    /// [`acquire_by_path`](Self::acquire_by_path) when the caller
    /// intends to hold the handle and must pair it with a
    /// `drop_texture`. See #524.
    pub fn get_by_path(&self, path: &str) -> Option<TextureHandle> {
        // 3 = WRAP_S_WRAP_T per nif.xml — the legacy REPEAT cache entry.
        self.get_by_path_with_clamp(path, 3)
    }

    /// `get_by_path`'s clamp-aware variant — looks up the cache entry
    /// for `(path, clamp_mode)`. Pre-#610 the cache was keyed by path
    /// alone; today the same path with two different clamp modes
    /// produces two distinct entries so the descriptor write picks the
    /// right `VkSamplerAddressMode`. Defaults (`clamp_mode == 0` =
    /// REPEAT) preserve the legacy single-key shape.
    pub fn get_by_path_with_clamp(&self, path: &str, clamp_mode: u8) -> Option<TextureHandle> {
        let clamp_mode = clamp_mode.min(3);
        self.path_map.get(&clamp_keyed_path(path, clamp_mode)).copied()
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
        // 3 = WRAP_S_WRAP_T per nif.xml — the legacy REPEAT cache entry.
        self.acquire_by_path_with_clamp(path, 3)
    }

    /// `acquire_by_path`'s clamp-aware variant. Same refcount semantics
    /// as the legacy entry point; the cache lookup includes
    /// `clamp_mode` so a non-zero clamp request resolves to its own
    /// entry instead of accidentally adopting the REPEAT-bound
    /// descriptor. See #610 / D4-NEW-02.
    pub fn acquire_by_path_with_clamp(
        &mut self,
        path: &str,
        clamp_mode: u8,
    ) -> Option<TextureHandle> {
        let clamp_mode = clamp_mode.min(3);
        let normalized = clamp_keyed_path(path, clamp_mode);
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

    /// Drain every entry's `pending_destroy` queue synchronously,
    /// regardless of `current_frame_id` aging. Counterpart of
    /// [`Self::tick_deferred_destroy`] for the shutdown path — caller
    /// must have already called `device_wait_idle` so queued textures
    /// can't be in-flight on any command buffer. See #732 / LIFE-H2.
    pub fn drain_pending_destroys(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for entry in &mut self.textures {
            for (_, mut pending) in entry.pending_destroy.drain(..) {
                pending.destroy(device, allocator);
            }
        }
    }

    /// Total number of textures still waiting across every entry's
    /// `pending_destroy` queue. Surfaced for the
    /// [`drain_pending_destroys`] regression test and shutdown
    /// telemetry. See #732.
    pub fn pending_destroy_count(&self) -> usize {
        self.textures
            .iter()
            .map(|e| e.pending_destroy.len())
            .sum()
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
        // #732 — factored the per-entry pending_destroy drain into
        // `drain_pending_destroys` so the App-level shutdown sweep can
        // call the same drain explicitly before `Drop`. Per-texture
        // teardown that follows still iterates `self.textures` directly
        // because it consumes each entry's `texture` Option.
        self.drain_pending_destroys(device, allocator);
        for entry in &mut self.textures {
            if let Some(mut t) = entry.texture.take() {
                t.destroy(device, allocator);
            }
        }
        self.textures.clear();
        self.path_map.clear();

        // Tear down the texture staging pool before the descriptor
        // set + sampler — the pool holds VkBuffers that must be
        // destroyed while the device is still valid. See #239.
        //
        // #732 LIFE-N1 — `take()` drops the `StagingPool` struct after
        // `destroy()` trims its free-list to zero, releasing the
        // pool's own `Arc<Mutex<Allocator>>` clone. Pre-fix the
        // `as_mut()` form left a populated `Some(StagingPool)` in
        // place; the Arc clone only released when `TextureRegistry`
        // itself naturally dropped at the tail of
        // `VulkanContext::Drop`, after `Arc::try_unwrap` had already
        // failed.
        if let Some(mut pool) = self.staging_pool.take() {
            pool.destroy();
        }

        unsafe {
            // #610 — destroy every clamp-mode sampler. `shared_sampler`
            // aliases `samplers[0]` so iterating the array covers it
            // too; don't double-destroy.
            for sampler in self.samplers {
                device.destroy_sampler(sampler, None);
            }
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

/// Cache key for the per-`(path, clamp_mode)` map (#610). Normalises
/// the path the same way as [`normalize_path`] then suffixes the
/// clamp-mode byte so the same texture path requested with different
/// `TexClampMode` values lands in separate entries — the underlying
/// GPU image is uploaded twice in that case but the descriptor binds
/// the right `VkSamplerAddressMode` pair. The default-REPEAT path
/// (`clamp_mode = 0`) keeps the legacy single-entry shape so existing
/// `acquire_by_path` / `get_by_path` (which look up the no-clamp key
/// implicitly) still hit.
fn clamp_keyed_path(path: &str, clamp_mode: u8) -> String {
    let mut s = normalize_path(path);
    s.push('|');
    s.push_str(&clamp_mode.to_string());
    s
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
            samplers: [vk::Sampler::null(); 4],
            max_textures,
            current_frame_id: 0,
            // Unit-test path: `check_slot_available` doesn't touch the
            // pool, so None is safe here.
            staging_pool: None,
            pending_set_writes: vec![Vec::new(); MAX_FRAMES_IN_FLIGHT],
            current_slot: 0,
            pending_dds_uploads: Vec::new(),
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
        // 3 = WRAP_S_WRAP_T per nif.xml — the legacy REPEAT entry the
        // pre-#610 cache lookups (`acquire_by_path` / `get_by_path`)
        // implicitly target.
        reg.path_map.insert(clamp_keyed_path(path, 3), 1);
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

    /// Regression for #610 / D4-NEW-02: the cache must distinguish
    /// `(path, clamp_mode)` so the same DDS requested with two
    /// different `TexClampMode` values gets two separate entries with
    /// the right `VkSamplerAddressMode` pair attached. Pre-#610 the
    /// cache was keyed by path alone — every texture got REPEAT and
    /// CLAMP-authored decals bled at edges.
    #[test]
    fn cache_separates_entries_by_clamp_mode() {
        let mut reg = make_registry_with_entry("chair.dds", 1);
        // Default `acquire_by_path` looks up the REPEAT (`3`) entry.
        assert_eq!(reg.acquire_by_path("chair.dds"), Some(1));
        assert_eq!(reg.textures[1].ref_count, 2);
        // Same path with `0 = CLAMP_S_CLAMP_T` is a different cache
        // entry — the seeded fixture has no entry under that key, so
        // the lookup MUST miss instead of accidentally adopting the
        // REPEAT-bound texture.
        assert_eq!(
            reg.acquire_by_path_with_clamp("chair.dds", 0),
            None,
            "CLAMP request must NOT alias to the REPEAT entry"
        );
        // The miss didn't touch the REPEAT entry's refcount.
        assert_eq!(reg.textures[1].ref_count, 2);
    }

    /// Sibling: `acquire_by_path_with_clamp(path, 3)` must hit the
    /// same entry the legacy `acquire_by_path` produced — the default
    /// arm is `WRAP_S_WRAP_T = 3` so existing call sites that don't
    /// pass a clamp keep their behaviour unchanged.
    #[test]
    fn legacy_acquire_path_routes_to_clamp_3() {
        let mut reg = make_registry_with_entry("chair.dds", 1);
        let h_legacy = reg.acquire_by_path("chair.dds");
        let h_explicit = reg.acquire_by_path_with_clamp("chair.dds", 3);
        assert_eq!(h_legacy, Some(1));
        assert_eq!(h_explicit, Some(1));
    }

    /// Sibling: out-of-range `clamp_mode` is clamped to `3` (REPEAT)
    /// in `acquire_by_path_with_clamp` — defensive default for
    /// upstream parser garbage.
    #[test]
    fn out_of_range_clamp_mode_falls_back_to_3() {
        let mut reg = make_registry_with_entry("chair.dds", 1);
        let h = reg.acquire_by_path_with_clamp("chair.dds", 99);
        assert_eq!(h, Some(1), "values >3 must clamp to the REPEAT entry");
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
        // #610 — path_map keys now suffix the clamp_mode (`|3` =
        // WRAP_S_WRAP_T, the default REPEAT). Pre-#610 the key was
        // `"textures/chair.dds"` alone.
        assert!(
            reg.path_map.contains_key("textures/chair.dds|3"),
            "cell B still holds a ref — path_map must survive"
        );
        assert!(
            reg.release_ref(1),
            "last release must authorise the GPU drop"
        );
        assert_eq!(reg.textures[1].ref_count, 0);
        assert!(
            !reg.path_map.contains_key("textures/chair.dds|3"),
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

    // ── #881 pending DDS upload queue mechanics ────────────────────

    /// Regression for #881 / CELL-PERF-03: the queueing core
    /// (`queue_or_hit`) reserves a fresh bindless slot on cache miss
    /// and pushes the upload onto the queue. The slot's `texture`
    /// stays `None` until `flush_pending_uploads` populates it; the
    /// queue mechanics — slot reservation, refcount = 1, path_map
    /// entry, queue length — are exercisable without an
    /// `ash::Device`. The fallback descriptor redirect on top of this
    /// (in `enqueue_dds_with_clamp`) is gated on the fallback
    /// entry's `texture` being `Some` — it's a no-op in production
    /// when the fallback is uninitialised, so the queueing core is
    /// what actually drives behaviour.
    #[test]
    fn enqueue_reserves_slot_and_queues_upload_on_miss() {
        let mut reg = make_registry_with_entry("placeholder.dds", 1);
        // Pre-state: 2 entries (fallback + the seeded `placeholder.dds`).
        assert_eq!(reg.textures.len(), 2);
        assert_eq!(reg.pending_dds_upload_count(), 0);

        // Miss path: `chair.dds` not in path_map → reserves slot 2.
        let bytes = vec![0u8; 128];
        let outcome = reg
            .queue_or_hit("chair.dds", bytes, 3)
            .expect("enqueue must succeed under non-overflow fixture");
        assert!(matches!(outcome, EnqueueOutcome::Reserved(2)));
        assert_eq!(reg.textures.len(), 3, "slot was pushed");
        assert!(
            reg.textures[2].texture.is_none(),
            "queued slot has no GPU image yet — flush populates it",
        );
        assert_eq!(
            reg.textures[2].ref_count, 1,
            "fresh enqueue starts at refcount 1 — symmetric with sync load_dds",
        );
        assert!(
            reg.path_map.contains_key("textures/chair.dds|3"),
            "path_map must point at the new handle so a sibling enqueue dedupes",
        );
        assert_eq!(reg.pending_dds_upload_count(), 1);
    }

    /// Repeat enqueue of the same `(path, clamp_mode)` pair must hit
    /// the path_map and bump the refcount instead of reserving a
    /// second slot — the cache-hit shape is the SAME as
    /// `acquire_by_path_with_clamp`.
    #[test]
    fn enqueue_cache_hit_bumps_refcount_no_queue_growth() {
        let mut reg = make_registry_with_entry("chair.dds", 1);
        // The seeded fixture entry sits at handle 1.
        let outcome = reg
            .queue_or_hit("chair.dds", vec![0u8; 8], 3)
            .expect("cache hit must not allocate");
        assert!(
            matches!(outcome, EnqueueOutcome::Hit(1)),
            "cache hit returns the existing handle without queueing",
        );
        assert_eq!(reg.textures[1].ref_count, 2, "refcount bumped");
        assert_eq!(
            reg.pending_dds_upload_count(),
            0,
            "cache hit must NOT enqueue (no upload work to do)",
        );
    }

    /// 100 distinct DDS files queue 100 pending uploads — the count
    /// is the number of `with_one_time_commands` calls
    /// `flush_pending_uploads` collapses into ONE submit. This is the
    /// invariant the audit's ~50–100 ms cell-load stall reduction
    /// depends on. Pre-#881 each enqueue would have paid its own
    /// fence-wait inline.
    #[test]
    fn one_hundred_uploads_queue_to_one_flush_batch() {
        let mut reg = make_registry_with_entry("placeholder.dds", 1);
        // Pad max_textures up to comfortably hold the test load.
        reg.max_textures = 256;

        for i in 0..100u32 {
            let path = format!("clutter_{i:03}.dds");
            let _ = reg
                .queue_or_hit(&path, vec![0u8; 64], 3)
                .expect("enqueue under non-overflow fixture must succeed");
        }
        assert_eq!(
            reg.pending_dds_upload_count(),
            100,
            "all 100 distinct paths must queue (the cell-load batched-flush invariant)",
        );
        // Sibling: 100 fresh slots reserved, all with `texture: None`.
        assert_eq!(reg.textures.len(), 102, "fallback + seed + 100 queued");
        for i in 2..102 {
            assert!(reg.textures[i].texture.is_none());
            assert_eq!(reg.textures[i].ref_count, 1);
        }
    }

    /// Sibling: `queue_or_hit` rejects when the bindless array is at
    /// the max bound. Mirrors the existing `slot_rejected_at_exact_bound`
    /// guard for the synchronous `load_dds_with_clamp` path. Without
    /// the rejection, an enqueue would push past the bindless array
    /// limit and corrupt descriptor state once the flush ran.
    #[test]
    fn enqueue_rejects_when_bindless_array_full() {
        let mut reg = make_registry_with_entry("placeholder.dds", 1);
        reg.max_textures = 2; // exact size the fixture has occupied.
        let err = reg
            .queue_or_hit("chair.dds", vec![0u8; 16], 3)
            .expect_err("full registry must refuse enqueue");
        let msg = format!("{err}");
        assert!(
            msg.contains("TextureRegistry is full"),
            "unexpected error: {msg}",
        );
        assert_eq!(reg.pending_dds_upload_count(), 0, "queue must stay empty on rejection");
    }
}
