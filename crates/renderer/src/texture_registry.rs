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
use crate::vulkan::sync::MAX_FRAMES_IN_FLIGHT;
use crate::vulkan::texture::Texture;
use anyhow::{Context, Result};
use ash::vk;
use std::collections::{HashMap, VecDeque};

/// Handle into the TextureRegistry (index into the bindless array).
pub type TextureHandle = u32;

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

/// Build the bindless texture descriptor-set-layout binding (set=0,
/// binding=0) consumed by `triangle.frag` + `ui.frag` + composite (the
/// raster pipelines). Pure data — no Vulkan device required — so the
/// `cargo test` reflection check can validate against the
/// include_bytes!'d SPIR-V before the first frame runs. Production
/// `TextureRegistry::new` routes through the same helper so test and
/// runtime can't drift. See #427 / #950.
pub(crate) fn build_bindless_descriptor_binding(
    max_textures: u32,
) -> vk::DescriptorSetLayoutBinding<'static> {
    vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(max_textures)
        .stage_flags(vk::ShaderStageFlags::FRAGMENT)
}

impl TextureRegistry {
    /// Reject a registration that would exceed the bindless array bound.
    ///
    /// Before #425 the array was sized to a hardcoded 1024 and callers
    /// would silently write past the bound once a cell loaded more unique
    /// textures, producing corrupted descriptor state or driver crashes.
    /// Now `max_textures` is driven by the device's
    /// `maxPerStageDescriptorUpdateAfterBindSampledImages` limit (clamped
    /// at 65535, the historical u16 bindless ceiling), and this check
    /// returns an error
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
        max_textures: u32,
        max_sampler_anisotropy: f32,
    ) -> Result<Self> {
        // Descriptor set layout: binding 0 = sampler2D[max_textures].
        // PARTIALLY_BOUND allows uninitialized array elements (the shader
        // only accesses indices that correspond to loaded textures).
        // UPDATE_AFTER_BIND allows writing new texture descriptors to a set
        // while a prior frame's command buffer still references it — safe
        // because only previously-unbound array indices are written.
        // #954 / REN-D3-NEW-01: VARIABLE_DESCRIPTOR_COUNT unlocks
        // allocating below `max_textures` if a future low-RAM startup
        // path wants a smaller bindless array. No behaviour change today
        // (the allocate-info matches the layout count); this is a
        // defence-in-depth addition that costs zero perf and unlocks
        // the variant for the future.
        let binding_flags = [vk::DescriptorBindingFlags::PARTIALLY_BOUND
            | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND
            | vk::DescriptorBindingFlags::VARIABLE_DESCRIPTOR_COUNT];
        let mut binding_flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&binding_flags);

        let binding = build_bindless_descriptor_binding(max_textures);

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
            make_sampler(
                vk::SamplerAddressMode::REPEAT,
                vk::SamplerAddressMode::REPEAT,
            )?,
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
        self.path_map
            .get(&clamp_keyed_path(path, clamp_mode))
            .copied()
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
        self.textures.iter().map(|e| e.pending_destroy.len()).sum()
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
#[path = "texture_registry_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "texture_registry_bindless_tests.rs"]
mod bindless_descriptor_reflection_tests;
