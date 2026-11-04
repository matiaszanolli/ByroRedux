//! Upload: the synchronous `load_dds*` path, the deferred `enqueue_dds*`
//! queue, and the per-frame `flush_pending_uploads` drain that turns
//! queued DDS bytes into resident bindless slots.
//!
//! `load_dds*` lives here rather than with lookup because it is the
//! synchronous twin of `enqueue_dds*` — same decode, same slot
//! allocation, a different point in the frame.

use super::residency::{self, Residency};
use super::*;

impl TextureRegistry {
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
        let ctx = GpuUploadCtx {
            device,
            allocator,
            queue,
            command_pool,
        };
        self.load_dds_with_clamp(ctx, path, dds_bytes, 3)
    }

    /// Load a DDS texture with an explicit Gamebryo `TexClampMode`
    /// (`0..=3`, see `samplers` field). Same caching + refcount
    /// semantics as [`Self::load_dds`]; the only behavioural difference
    /// is the sampler bound to the bindless descriptor entry.
    ///
    /// `clamp_mode` values outside `0..=3` are clamped **up** to `3`
    /// (`WRAP_S_WRAP_T` = REPEAT/REPEAT, nif.xml's own default) — a
    /// defensive default for upstream parser garbage that lands on the
    /// legacy behaviour rather than on `0`, which is `CLAMP_S_CLAMP_T`.
    /// Pinned by `out_of_range_clamp_mode_falls_back_to_3`.
    ///
    /// #3757 — this used to read "clamped to `0` (REPEAT)", wrong twice
    /// over: wrong about which index is REPEAT (it is 3; see the
    /// `samplers` field's own table) and wrong about the direction
    /// (`clamp_mode.min(3)` clamps up, not to zero).
    ///
    /// Cache key includes `clamp_mode`: the same `path` requested with
    /// two different clamp modes produces two distinct entries (the
    /// underlying GPU image is uploaded twice — acceptable since this
    /// is rare in Bethesda content; per-material clamp_mode is the
    /// almost-universal authoring pattern). See #610 / D4-NEW-02.
    pub fn load_dds_with_clamp(
        &mut self,
        ctx: GpuUploadCtx,
        path: &str,
        dds_bytes: &[u8],
        clamp_mode: u8,
    ) -> Result<TextureHandle> {
        self.load_dds_with_clamp_and_color_space(
            ctx,
            path,
            dds_bytes,
            clamp_mode,
            TextureColorSpace::Srgb,
        )
    }

    /// Role-aware DDS load. The cache key includes `color_space`, so one GPU
    /// upload is shared by all users of the same `(path, clamp, semantic)`;
    /// a second image is allocated only if content genuinely reuses one path
    /// as both a colour texture and a numeric data map.
    pub fn load_dds_with_clamp_and_color_space(
        &mut self,
        ctx: GpuUploadCtx,
        path: &str,
        dds_bytes: &[u8],
        clamp_mode: u8,
        color_space: TextureColorSpace,
    ) -> Result<TextureHandle> {
        let clamp_mode = clamp_mode.min(3);
        let meta = crate::vulkan::dds::parse_dds_with_color_space(dds_bytes, color_space)?;
        anyhow::ensure!(
            !meta.is_cubemap,
            "DDS '{}' is a cubemap; use the environment-map loader",
            path,
        );
        let normalized =
            texture_keyed_path_with_color_space(path, clamp_mode, TextureViewKind::D2, color_space);

        if let Some(&handle) = self.path_map.get(&normalized) {
            if let Some(entry) = self.textures.get_mut(handle as usize) {
                entry.ref_count = entry.ref_count.saturating_add(1);
            }
            return Ok(handle);
        }

        // Reject before paying the upload cost if the bindless array is full.
        self.check_slot_available()?;

        let device = ctx.device;
        let pixel_data = crate::vulkan::dds::upload_pixels(&meta, dds_bytes);
        let texture = Texture::from_dds_with_mip_chain(
            ctx,
            &meta,
            pixel_data.as_ref(),
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
            view_kind: TextureViewKind::D2,
            has_alpha: crate::vulkan::dds::format_has_alpha(meta.format),
            avg_rgb: None,
            residency: None,
        });
        self.path_map.insert(normalized, handle);
        // #1542: for a to-be-expanded 16/24-bpp source the raw bytes aren't
        // RGBA8 yet, so `average_rgb` would misread them; skip. Numeric slots
        // likewise never feed the diffuse GI-albedo cache.
        if color_space == TextureColorSpace::Srgb && meta.expand.is_none() {
            if let Some(avg) = crate::vulkan::dds::average_rgb(&meta, dds_bytes) {
                if let Some(entry) = self.textures.get_mut(handle as usize) {
                    entry.avg_rgb = Some(avg);
                }
            }
        }

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
        self.enqueue_dds_with_clamp_and_color_space(
            device,
            path,
            dds_bytes,
            clamp_mode,
            TextureColorSpace::Srgb,
        )
    }

    /// Batched counterpart of [`Self::load_dds_with_clamp_and_color_space`].
    pub fn enqueue_dds_with_clamp_and_color_space(
        &mut self,
        device: &ash::Device,
        path: &str,
        dds_bytes: Vec<u8>,
        clamp_mode: u8,
        color_space: TextureColorSpace,
    ) -> Result<TextureHandle> {
        let meta = crate::vulkan::dds::parse_dds_with_color_space(&dds_bytes, color_space)?;
        anyhow::ensure!(
            !meta.is_cubemap,
            "DDS '{}' is a cubemap; use enqueue_cubemap_dds_with_clamp",
            path,
        );
        self.enqueue_dds_for_view(
            device,
            path,
            dds_bytes,
            clamp_mode,
            TextureViewKind::D2,
            color_space,
        )
    }

    /// Queue a six-face DDS environment map for the bindless samplerCube
    /// binding. Validation happens before reserving a handle, so a legacy 2D
    /// sphere map cannot be consumed through an incompatible cube descriptor.
    pub fn enqueue_cubemap_dds_with_clamp(
        &mut self,
        device: &ash::Device,
        path: &str,
        dds_bytes: Vec<u8>,
        clamp_mode: u8,
    ) -> Result<TextureHandle> {
        let meta = crate::vulkan::dds::parse_dds(&dds_bytes)?;
        anyhow::ensure!(meta.is_cubemap, "DDS '{}' is not a six-face cubemap", path);
        self.enqueue_dds_for_view(
            device,
            path,
            dds_bytes,
            clamp_mode,
            TextureViewKind::Cube,
            TextureColorSpace::Srgb,
        )
    }

    fn enqueue_dds_for_view(
        &mut self,
        device: &ash::Device,
        path: &str,
        dds_bytes: Vec<u8>,
        clamp_mode: u8,
        view_kind: TextureViewKind,
        color_space: TextureColorSpace,
    ) -> Result<TextureHandle> {
        let outcome =
            self.queue_or_hit_for_view(path, dds_bytes, clamp_mode, view_kind, color_space)?;
        // For a fresh enqueue, redirect the freshly-reserved
        // descriptor to the fallback checkerboard. Any
        // GpuInstance.texture_index that resolves to this handle
        // before the flush samples the checkerboard instead of an
        // unbound descriptor — same defence `drop_texture` uses on
        // the release side. Cache hits skip this step (the existing
        // entry already has its real descriptor wired).
        if view_kind == TextureViewKind::D2 && matches!(outcome, EnqueueOutcome::Reserved(_)) {
            let fallback_idx = self.fallback_handle as usize;
            if fallback_idx < self.textures.len() {
                if let Some(fallback) = self.textures[fallback_idx].texture.as_ref() {
                    let image_view = fallback.image_view;
                    let sampler = fallback.sampler;
                    let handle = outcome.handle();
                    self.apply_descriptor_write(device, handle, 0, image_view, sampler);
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
    #[cfg(test)]
    pub(super) fn queue_or_hit(
        &mut self,
        path: &str,
        dds_bytes: Vec<u8>,
        clamp_mode: u8,
    ) -> Result<EnqueueOutcome> {
        self.queue_or_hit_for_view(
            path,
            dds_bytes,
            clamp_mode,
            TextureViewKind::D2,
            TextureColorSpace::Srgb,
        )
    }

    pub(super) fn queue_or_hit_for_view(
        &mut self,
        path: &str,
        dds_bytes: Vec<u8>,
        clamp_mode: u8,
        view_kind: TextureViewKind,
        color_space: TextureColorSpace,
    ) -> Result<EnqueueOutcome> {
        let clamp_mode = clamp_mode.min(3);
        let normalized =
            texture_keyed_path_with_color_space(path, clamp_mode, view_kind, color_space);

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
            // #4885 — retained so the resize rewrite knows which binding
            // the reserved slot must be fallback-redirected through.
            view_kind,
            // Populated by `flush_pending_uploads` once the DDS header is
            // parsed; `false`/`None` until then, matching the pre-#3682
            // HashMap's "absent handle reads false/None" contract.
            has_alpha: false,
            avg_rgb: None,
            residency: None,
        });
        self.path_map.insert(normalized, handle);
        self.pending_dds_uploads.push(PendingDdsUpload {
            handle,
            staged_bytes: crate::vulkan::dds::staged_bytes(&dds_bytes),
            dds_bytes,
            clamp_mode,
            view_kind,
            color_space,
            path: path.to_string(),
            change: None,
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

    /// M39 — the `render.texture_streaming` switch (TXS-RES-008). Takes
    /// effect on the next first upload; textures already resident keep
    /// their state.
    pub fn set_streaming(&mut self, on: bool) {
        self.streaming = on;
    }

    /// M39 — plan a promotion or demotion of `handle` that makes `target`
    /// its first resident level, and queue it. `dds_bytes` is a DDS file
    /// (full-chain header, payload from any level at or above the change's
    /// new first level; a demotion needs only the header). `None` when the
    /// handle is not a live streamable texture, nothing would move, or the
    /// bytes cannot supply the levels a promotion uploads.
    pub(crate) fn queue_residency_change(
        &mut self,
        handle: TextureHandle,
        target: u32,
        dds_bytes: Vec<u8>,
        clamp_mode: u8,
        color_space: TextureColorSpace,
        path: String,
    ) -> Option<ResidencyChange> {
        let entry = self.textures.get(handle as usize)?;
        if entry.ref_count == 0 {
            return None;
        }
        let change = entry.residency?.plan(target)?;
        let staged_bytes = if change.is_promotion() {
            let meta =
                crate::vulkan::dds::parse_dds_with_color_space(&dds_bytes, color_space).ok()?;
            let payload = dds_bytes.len().saturating_sub(meta.data_offset);
            if crate::vulkan::dds::payload_first_level(&meta, payload) > change.new_first {
                return None;
            }
            let range = crate::vulkan::dds::level_byte_range(&meta, change.upload_levels());
            (range.end - range.start) as vk::DeviceSize
        } else {
            0
        };
        self.pending_promotions.push(PendingDdsUpload {
            handle,
            dds_bytes,
            staged_bytes,
            clamp_mode,
            view_kind: TextureViewKind::D2,
            color_space,
            path,
            change: Some(change),
        });
        Some(change)
    }

    /// M39 — whether first uploads stream (the `render.texture_streaming`
    /// switch); the host reads floors only while it is on.
    pub fn streaming(&self) -> bool {
        self.streaming
    }

    /// M39 — up to `max` promotions waiting for the host to read their
    /// levels, oldest first.
    pub fn take_promotion_requests(&mut self, max: usize) -> Vec<PromotionRequest> {
        let take = max.min(self.promotion_requests.len());
        let taken: Vec<(TextureHandle, u32)> = self.promotion_requests.drain(..take).collect();
        taken
            .into_iter()
            .filter_map(|(handle, target)| {
                let source = self.stream_sources.get(&handle)?;
                Some(PromotionRequest {
                    handle,
                    path: source.path.to_string(),
                    target,
                })
            })
            .collect()
    }

    /// M39 — level 0's larger edge of a streamed texture, `None` for one that
    /// is not streamed (the wanted-mip pass skips it).
    pub fn stream_width(&self, handle: TextureHandle) -> Option<u32> {
        self.stream_sources.get(&handle).map(|source| source.width)
    }

    /// M39 — apply this frame's wanted mips (docs/engine/texture-streaming.md
    /// §5–§6). Every streamed texture absent from `wanted` is wanted at its
    /// floor (TXS-WANT-003). A texture wanted finer than it is resident
    /// becomes a promotion request, ordered drawn before TLAS-only before
    /// neither, then larger deficit, then nearer (TXS-WANT-004); the list
    /// replaces last frame's. One wanted coarser for `DEMOTE_AFTER_FRAMES`
    /// frames, and as long after its last promotion, is demoted
    /// (TXS-WANT-005). A texture with a change already queued waits.
    pub fn apply_wanted(
        &mut self,
        wanted: &HashMap<TextureHandle, residency::WantedMip>,
    ) -> WantedStats {
        use residency::{WantedCase, DEMOTE_AFTER_FRAMES};
        let frame = self.current_frame_id;
        let queued: HashSet<TextureHandle> = self
            .pending_promotions
            .iter()
            .map(|upload| upload.handle)
            .collect();
        let mut stats = WantedStats::default();
        let mut candidates: Vec<(WantedCase, u32, f32, TextureHandle, u32)> = Vec::new();
        let mut demotions = Vec::new();
        for (&handle, source) in self.stream_sources.iter_mut() {
            let Some(r) = self
                .textures
                .get(handle as usize)
                .and_then(|entry| entry.residency)
            else {
                continue;
            };
            let w = wanted
                .get(&handle)
                .copied()
                .unwrap_or(residency::WantedMip {
                    level: r.floor_first,
                    case: WantedCase::Neither,
                    distance: f32::INFINITY,
                });
            let target = w.level.min(r.floor_first);
            source.wanted = Some((target, w.case));
            if w.case != WantedCase::Neither {
                source.last_wanted_frame = frame;
            }
            if target < r.first_resident {
                stats.deficits += 1;
                source.coarser_since = None;
                if !queued.contains(&handle) {
                    candidates.push((
                        w.case,
                        r.first_resident - target,
                        w.distance,
                        handle,
                        target,
                    ));
                }
            } else if target > r.first_resident {
                let since = *source.coarser_since.get_or_insert(frame);
                let settled = frame.wrapping_sub(since) >= DEMOTE_AFTER_FRAMES
                    && source
                        .last_promoted_frame
                        .is_none_or(|promoted| frame.wrapping_sub(promoted) >= DEMOTE_AFTER_FRAMES);
                if settled && !queued.contains(&handle) {
                    demotions.push((handle, target));
                    source.coarser_since = None;
                }
            } else {
                source.coarser_since = None;
            }
        }
        candidates.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(a.2.total_cmp(&b.2)));
        stats.requests = candidates.len() as u32;
        self.promotion_requests = candidates
            .into_iter()
            .map(|(_, _, _, handle, target)| (handle, target))
            .collect();
        for (handle, target) in demotions {
            if self.queue_demotion(handle, target) {
                stats.demotions += 1;
            }
        }
        stats
    }

    /// M39 — queue a demotion of a streamed texture to first resident level
    /// `target` (never coarser than its floor). A demotion uploads nothing,
    /// so the source's DDS header is all the queued change carries.
    /// `false` when the handle is not streamed or nothing would move.
    pub fn queue_demotion(&mut self, handle: TextureHandle, target: u32) -> bool {
        let Some(source) = self.stream_sources.get(&handle).cloned() else {
            return false;
        };
        let demotes = self
            .textures
            .get(handle as usize)
            .and_then(|entry| entry.residency)
            .is_some_and(|residency| target > residency.first_resident);
        demotes
            && self
                .queue_residency_change(
                    handle,
                    target,
                    source.header,
                    source.clamp_mode,
                    source.color_space,
                    source.path.to_string(),
                )
                .is_some()
    }

    /// M39 — queue the change a [`PromotionRequest`] asked for, from the
    /// bytes the host read (a DDS file whose payload starts at or above the
    /// request's target). `false` when the handle was released or has moved
    /// so nothing is left to do, or the bytes do not cover the target.
    pub fn queue_promotion_bytes(
        &mut self,
        handle: TextureHandle,
        target: u32,
        dds_bytes: Vec<u8>,
    ) -> bool {
        let Some(source) = self.stream_sources.get(&handle).cloned() else {
            return false;
        };
        self.queue_residency_change(
            handle,
            target,
            dds_bytes,
            source.clamp_mode,
            source.color_space,
            source.path.to_string(),
        )
        .is_some()
    }

    /// M39 — queued promotions and demotions, for telemetry.
    pub fn pending_promotion_count(&self) -> usize {
        self.pending_promotions.len()
    }

    /// M39 TXS-SRC-004 — count one floor install by what the archive layer
    /// handed over: only the tail, or the whole file.
    pub(crate) fn note_floor_install(&mut self, from_whole_file: bool) {
        if from_whole_file {
            self.stream_counters.floors_from_whole_file += 1;
        } else {
            self.stream_counters.floors_from_tail += 1;
        }
    }

    /// M39 — record a residency change that just landed: the hysteresis
    /// frames `apply_wanted` reads (TXS-WANT-005) and the session counters
    /// (TXS-TEL-002). A promotion and a demotion of one texture landing
    /// within `DEMOTE_AFTER_FRAMES` of each other, either order, is churn.
    pub(crate) fn note_change_landed(&mut self, handle: TextureHandle, change: ResidencyChange) {
        use residency::DEMOTE_AFTER_FRAMES;
        let frame = self.current_frame_id;
        let within = |other: Option<u64>| {
            other.is_some_and(|other| frame.wrapping_sub(other) < DEMOTE_AFTER_FRAMES)
        };
        let counters = &mut self.stream_counters;
        let source = self.stream_sources.get_mut(&handle);
        if change.is_promotion() {
            counters.promotions += 1;
            if let Some(source) = source {
                counters.churn += u64::from(within(source.last_demoted_frame));
                source.last_promoted_frame = Some(frame);
            }
        } else {
            counters.demotions += 1;
            if let Some(source) = source {
                counters.churn += u64::from(within(source.last_promoted_frame));
                source.last_demoted_frame = Some(frame);
            }
        }
    }

    /// M39 TXS-TEL-002 — record a cell load's floor-ready time once its last
    /// texture flush has run: every texture the load reserved has left
    /// **reserved** (PRD §8). `started` is the load's own clock, taken after
    /// its record parse (which reads no texture and costs the same with
    /// streaming on and off). With streaming off this times the full-chain
    /// flush, the A6 control. `None` (nothing recorded) while uploads are
    /// still queued.
    pub fn record_cell_load_ready(&mut self, started: std::time::Instant) -> Option<f64> {
        if !self.pending_dds_uploads.is_empty() {
            return None;
        }
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        self.cell_loads_timed += 1;
        self.last_floor_ready_ms = elapsed_ms;
        Some(elapsed_ms)
    }

    /// M39 TXS-TEL-002 — cell loads `record_cell_load_ready` has timed; a
    /// change restarts the host's settle clock.
    pub fn cell_loads_timed(&self) -> u64 {
        self.cell_loads_timed
    }

    /// M39 — whether no promotion is requested of the host or queued for
    /// upload: the registry's half of "settled" (PRD §8 settle time; the
    /// host adds its reads in flight).
    pub fn promotions_idle(&self) -> bool {
        self.promotion_requests.is_empty()
            && self
                .pending_promotions
                .iter()
                .all(|upload| upload.change.is_none_or(|change| !change.is_promotion()))
    }

    /// M39 — refresh what `tex.residency` and the `tex-stream:` bench line
    /// print (TXS-TEL-001/002). One pass over the slots, every frame (so the
    /// session peak sees each frame's total), reusing `out`'s per-handle
    /// buffer.
    pub fn fill_residency_telemetry(
        &mut self,
        out: &mut byroredux_core::ecs::TextureResidencyTelemetry,
    ) {
        let mut t = byroredux_core::ecs::TextureResidencyTelemetry {
            streaming: self.streaming,
            queued_changes: self.pending_promotions.len() as u64,
            promotion_requests: self.promotion_requests.len() as u64,
            promotions: self.stream_counters.promotions,
            demotions: self.stream_counters.demotions,
            churn: self.stream_counters.churn,
            floors_from_tail: self.stream_counters.floors_from_tail,
            floors_from_whole_file: self.stream_counters.floors_from_whole_file,
            hysteresis_frames: residency::DEMOTE_AFTER_FRAMES,
            cell_loads_timed: self.cell_loads_timed,
            floor_ready_ms: self.last_floor_ready_ms,
            // Refreshed every frame: keep last frame's allocation.
            per_handle: {
                let mut per_handle = std::mem::take(&mut out.per_handle);
                per_handle.clear();
                per_handle
            },
            paths: {
                let mut paths = std::mem::take(&mut out.paths);
                paths.clear();
                paths
            },
            frame: self.current_frame_id,
            last_wanted_frames: {
                let mut frames = std::mem::take(&mut out.last_wanted_frames);
                frames.clear();
                frames
            },
            ..Default::default()
        };
        let add = |slot: &mut (u64, u64), bytes: u64| {
            slot.0 += 1;
            slot.1 += bytes;
        };
        for (slot, entry) in self.textures.iter().enumerate() {
            t.retiring_bytes += entry
                .pending_destroy
                .iter()
                .map(|(_, texture)| texture.allocation_bytes())
                .sum::<u64>();
            let Some(texture) = entry.texture.as_ref() else {
                if entry.ref_count > 0 {
                    t.reserved += 1;
                }
                t.per_handle.push(None);
                t.paths.push(None);
                t.last_wanted_frames.push(0);
                continue;
            };
            let bytes = texture.allocation_bytes();
            t.resident_bytes += bytes;
            match entry.residency {
                Some(r) => {
                    t.floor_bytes += r.floor_bytes;
                    let state = if r.first_resident == 0 {
                        &mut t.full
                    } else if r.first_resident == r.floor_first {
                        &mut t.floor
                    } else {
                        &mut t.partial
                    };
                    add(state, bytes);
                    debug_assert_eq!(r.resident_levels(), r.mip_count - r.first_resident);
                    let (wanted, case) = self
                        .stream_sources
                        .get(&(slot as TextureHandle))
                        .and_then(|source| source.wanted)
                        .map_or((u32::MAX, u32::MAX), |(level, case)| (level, case as u32));
                    if wanted != u32::MAX && wanted < r.first_resident {
                        t.deficits += 1;
                    }
                    t.per_handle.push(Some([
                        r.first_resident,
                        r.floor_first,
                        r.mip_count,
                        wanted,
                        case,
                    ]));
                    let source = self.stream_sources.get(&(slot as TextureHandle));
                    t.paths
                        .push(source.map(|source| std::sync::Arc::clone(&source.path)));
                    t.last_wanted_frames
                        .push(source.map_or(0, |source| source.last_wanted_frame));
                }
                None => {
                    let class = if texture.view_kind == TextureViewKind::Cube {
                        &mut t.whole_cube
                    } else if texture.level_count() <= 1 {
                        &mut t.whole_single_level
                    } else {
                        &mut t.whole_chain
                    };
                    add(class, bytes);
                    t.per_handle.push(None);
                    t.paths.push(None);
                    t.last_wanted_frames.push(0);
                }
            }
        }
        self.peak_resident_bytes = self.peak_resident_bytes.max(t.resident_bytes);
        t.peak_resident_bytes = self.peak_resident_bytes;
        *out = t;
    }

    /// M39 — flush queued promotions and demotions in queue order, staging
    /// at most `max_bytes` (at least one change per call, as one oversized
    /// upload cannot be split), through the same sub-batch path as first
    /// uploads (TXS-UP-004). Called once per frame.
    pub fn flush_pending_promotions(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        transfer_fence: &std::sync::Mutex<vk::Fence>,
        max_bytes: vk::DeviceSize,
    ) -> Result<usize> {
        let take = promotion_take_count(&self.pending_promotions, max_bytes);
        if take == 0 {
            return Ok(0);
        }
        let batch: Vec<PendingDdsUpload> = self.pending_promotions.drain(..take).collect();
        self.flush_upload_batch(
            device,
            allocator,
            queue,
            command_pool,
            transfer_fence,
            batch,
        )
    }

    /// Total bytes the queued DDS uploads will stage — the figure the cell
    /// loader's yield trigger compares against [`MAX_UPLOAD_BATCH_BYTES`]
    /// (#4197). Priced by [`crate::vulkan::dds::staged_bytes`], the same
    /// figure the flush budgets its sub-batches by, so it is the expanded
    /// size for a 16/24-bpp source rather than the file's (#4835).
    pub fn pending_dds_upload_bytes(&self) -> vk::DeviceSize {
        queued_upload_sizes(&self.pending_dds_uploads).iter().sum()
    }

    /// Drain the queued DDS uploads with one batched submit + fence-wait
    /// per sub-batch of at most [`MAX_UPLOAD_BATCH_BYTES`] (#4197).
    /// Pre-#881 each `Texture::from_dds_with_mip_chain`
    /// paid its own `vkQueueSubmit` + `vkWaitForFences(.., u64::MAX)`,
    /// so a worldspace edge crossing with 100 fresh DDS textures
    /// burned ~100 sync stalls (~50–100 ms) on the main thread. The
    /// queueing path collapses those to one stall per byte-bounded
    /// sub-batch — usually a single one, since a cell's queue rarely
    /// reaches the bound.
    ///
    /// Returns the number of textures uploaded (≥ 0). On any
    /// recording error the queue is taken and dropped, not preserved
    /// for retry — sub-batches that already completed stay installed, but
    /// every entry in the failed sub-batch and every later one is gone, and any
    /// handle already reserved for it (via `queue_or_hit`) stays
    /// `texture: None` forever, cache-HIT-redirected to a dead handle
    /// until every reference to it drops (see #1922 for the fix-vs-
    /// document tradeoff). A failure before `vkQueueSubmit` frees the
    /// partial command buffer without submitting and destroys the failed
    /// sub-batch's staged images and staging buffers at once. On a submit or
    /// fence-wait error the GPU may still be reading them, so they are
    /// leaked instead — never returned to the pool, never destroyed (#4891;
    /// the same policy as `GpuBuffer::create_device_local_buffers_batched`).
    /// A future-proof alternative would defer-destroy them, but that error
    /// class is device loss, already a fatal-style path.
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
        // record loop without aliasing the field. On a recording
        // error the rest of the taken queue is simply dropped — there
        // is no push-back, so any entries not yet staged at the point
        // of failure are lost, not retried (#1922).
        let pending = std::mem::take(&mut self.pending_dds_uploads);
        let count = pending.len();

        // #4197 — every staging buffer in one submit stays live until that
        // submit's fence wait returns, and `StagingPool` bounds only what it
        // retains. Drain in sub-batches of at most `MAX_UPLOAD_BATCH_BYTES`;
        // each releases its staging back to the pool before the next one
        // acquires, so peak staging is one sub-batch rather than the queue.
        // #4835 — sized by what each upload STAGES, not by its file length: a
        // 128-byte 16/24-bpp header expands to hundreds of MB, and weighing it
        // at 128 B let the queue exceed the bound this split exists to keep.
        let sizes = queued_upload_sizes(&pending);
        let batches = upload_batch_ranges(&sizes, MAX_UPLOAD_BATCH_BYTES);
        let batch_count = batches.len();
        let mut remaining = pending.into_iter();
        let mut uploaded = 0;
        for (index, range) in batches.into_iter().enumerate() {
            let batch: Vec<PendingDdsUpload> = remaining.by_ref().take(range.len()).collect();
            match self.flush_upload_batch(
                device,
                allocator,
                queue,
                command_pool,
                transfer_fence,
                batch,
            ) {
                Ok(staged) => uploaded += staged,
                Err(e) => {
                    let dropped = remaining.len();
                    if dropped > 0 {
                        log::warn!(
                            "flush_pending_uploads: sub-batch {}/{batch_count} failed; \
                             {dropped} later queued uploads dropped with it",
                            index + 1,
                        );
                    }
                    return Err(e);
                }
            }
        }

        log::info!(
            "Flushed {uploaded} queued DDS uploads ({count} originally enqueued, \
             {batch_count} sub-batch(es))",
        );
        Ok(uploaded)
    }

    /// Record, submit, fence-wait and install one sub-batch of queued
    /// uploads for [`Self::flush_pending_uploads`].
    fn flush_upload_batch(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        transfer_fence: &std::sync::Mutex<vk::Fence>,
        pending: Vec<PendingDdsUpload>,
    ) -> Result<usize> {
        let count = pending.len();

        // Per-upload outputs assembled during recording; consumed
        // after the submit + wait completes.
        struct StagedUpload {
            handle: TextureHandle,
            texture: crate::vulkan::texture::Texture,
            staging: Option<crate::vulkan::buffer::StagingGuard>,
            /// M39 — `Some` when this replaces a live streamable image.
            change: Option<ResidencyChange>,
            /// M39 — the residency a floor-first upload installs, whether
            /// its source bytes hold the whole chain, the index of that
            /// source in `pending`, and its header length.
            floor: Option<(Residency, bool, usize, usize)>,
        }
        let mut staged: Vec<StagedUpload> = Vec::with_capacity(count);

        // Use the persistent transfer fence (#302) so we don't pay a
        // per-flush vk::Fence create/destroy. `with_one_time_commands_reuse_fence`
        // resets + locks the fence for the duration.
        let record_result = crate::vulkan::texture::with_one_time_commands_reuse_fence(
            device,
            queue,
            command_pool,
            transfer_fence,
            |cmd| {
                for (index, upload) in pending.iter().enumerate() {
                    // #4879 — the release path purges queued uploads for
                    // freed handles, but a slot whose last reference is
                    // already gone must never be installed: `ref_count == 0`
                    // means `decrement_ref` will refuse every future
                    // release, so the texture would orphan for the process
                    // lifetime. Skip before paying the parse/staging cost.
                    if self
                        .textures
                        .get(upload.handle as usize)
                        .is_some_and(|entry| entry.ref_count == 0)
                    {
                        log::warn!(
                            "Skipping queued DDS upload '{}': handle {} was released before flush",
                            upload.path,
                            upload.handle,
                        );
                        continue;
                    }
                    let meta = match crate::vulkan::dds::parse_dds_with_color_space(
                        &upload.dds_bytes,
                        upload.color_space,
                    ) {
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
                    let parsed_view_kind = if meta.is_cubemap {
                        TextureViewKind::Cube
                    } else {
                        TextureViewKind::D2
                    };
                    if parsed_view_kind != upload.view_kind {
                        log::warn!(
                            "DDS '{}' changed view kind between queue and flush ({:?} -> {:?}); dropping upload",
                            upload.path,
                            upload.view_kind,
                            parsed_view_kind,
                        );
                        continue;
                    }
                    if let Some(entry) = self.textures.get_mut(upload.handle as usize) {
                        entry.has_alpha = crate::vulkan::dds::format_has_alpha(meta.format);
                    }
                    let pixel_data = crate::vulkan::dds::upload_pixels(&meta, &upload.dds_bytes);
                    let sampler = self.samplers[upload.clamp_mode as usize];
                    // M39 — a BA2 floor read keeps the full-chain header over
                    // a payload that starts at its last chunk's level.
                    let payload_first = if meta.array_layers == 1 && meta.expand.is_none() {
                        crate::vulkan::dds::payload_first_level(&meta, pixel_data.len())
                    } else {
                        0
                    };

                    // M39 — a promotion or demotion of a live streamable
                    // texture: only if it still holds the range the change
                    // was planned against (a later change or a release may
                    // have moved it since).
                    if let Some(change) = upload.change {
                        let old_image =
                            self.textures.get(upload.handle as usize).and_then(|entry| {
                                let current = entry.residency?;
                                let texture = entry.texture.as_ref()?;
                                (current.first_resident == change.old_first)
                                    .then_some(texture.image)
                            });
                        let Some(old_image) = old_image else {
                            log::debug!(
                                "Skipping stale residency change for '{}' ({change:?})",
                                upload.path,
                            );
                            continue;
                        };
                        let upload_levels = change.upload_levels();
                        let bytes: &[u8] = if upload_levels.is_empty() {
                            &[]
                        } else if upload_levels.start < payload_first {
                            log::warn!(
                                "Residency change for '{}' needs level {} but its bytes start at {}; dropping it",
                                upload.path,
                                upload_levels.start,
                                payload_first,
                            );
                            continue;
                        } else {
                            &pixel_data[crate::vulkan::dds::level_byte_range_in(
                                &meta,
                                upload_levels,
                                payload_first,
                            )]
                        };
                        match crate::vulkan::texture::Texture::record_dds_residency_change(
                            device,
                            allocator,
                            cmd,
                            &meta,
                            change,
                            old_image,
                            bytes,
                            sampler,
                            self.staging_pool.as_mut(),
                        ) {
                            Ok((texture, staging)) => staged.push(StagedUpload {
                                handle: upload.handle,
                                texture,
                                staging,
                                change: Some(change),
                                floor: None,
                            }),
                            Err(e) => log::warn!(
                                "Failed to record residency change for '{}': {} — dropping it",
                                upload.path,
                                e,
                            ),
                        }
                        continue;
                    }

                    // M39 — with streaming on, a streamable 2D texture's
                    // first upload installs only its floor (TXS-RES-001).
                    // 16/24-bpp sources expanded on the CPU stay whole.
                    // Bytes that start above level 0 (a BA2 floor read) can
                    // only install as a floor, whatever the switch says now.
                    let floor_first = if payload_first > 0 {
                        Some(payload_first)
                    } else {
                        (self.streaming
                            && upload.view_kind == TextureViewKind::D2
                            && !meta.is_cubemap
                            && meta.array_layers == 1
                            && meta.expand.is_none())
                        .then(|| {
                            residency::floor_first_level(meta.width, meta.height, meta.mip_count)
                        })
                        .flatten()
                    };
                    let first_mip = floor_first.unwrap_or(0);
                    let levels = crate::vulkan::dds::upload_byte_range(
                        &meta,
                        first_mip,
                        payload_first,
                        pixel_data.len(),
                    );
                    // #1542: a 16/24-bpp `DDPF_RGB` source is CPU-expanded to
                    // R8G8B8A8. Its raw bytes aren't RGBA8 yet, so skip
                    // `average_rgb` for it (it would misread the packed
                    // pixels) — these are UI/font atlases, never diffuse
                    // albedo anyway. The average reads the smallest level,
                    // which a floor holds (TXS-RES-006).
                    if upload.color_space == TextureColorSpace::Srgb && meta.expand.is_none() {
                        if let Some(avg) = crate::vulkan::dds::average_rgb_levels(
                            &meta,
                            &pixel_data[levels.clone()],
                            first_mip,
                        ) {
                            if let Some(entry) = self.textures.get_mut(upload.handle as usize) {
                                entry.avg_rgb = Some(avg);
                            }
                        }
                    }
                    let (texture, staging) =
                        match crate::vulkan::texture::Texture::record_dds_upload(
                            device,
                            allocator,
                            cmd,
                            &meta,
                            first_mip,
                            floor_first.is_some(),
                            &pixel_data[levels],
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
                    let floor = floor_first.map(|floor_first| {
                        let floor = crate::vulkan::dds::level_byte_range(
                            &meta,
                            floor_first..meta.mip_count,
                        );
                        (
                            Residency::at_floor(
                                meta.mip_count,
                                floor_first,
                                (floor.end - floor.start) as u64,
                            ),
                            payload_first == 0,
                            index,
                            meta.data_offset,
                        )
                    });
                    staged.push(StagedUpload {
                        handle: upload.handle,
                        texture,
                        staging: Some(staging),
                        change: None,
                        floor,
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
            // The pending queue is gone (we `take`d it) — but `path_map`
            // was already populated by `queue_or_hit` at enqueue time, so a
            // later request for the same path cache-HITs the dead
            // `texture: None` handle instead of re-queuing (#1922).
            log::warn!(
                "flush_pending_uploads submission failed ({} uploads dropped): {}",
                staged.len(),
                e,
            );
            if crate::vulkan::texture::OneTimeCommandError::may_be_in_flight(&e) {
                // #4891 — submit or fence wait failed: the copies may still
                // be executing, so a host-side destroy could race them. Leak
                // both sides, as the batched buffer path does.
                std::mem::forget(staged);
            } else {
                // Nothing was submitted: destroy the partially-staged
                // textures so their VkImage / staging buffer don't leak.
                for mut s in staged {
                    s.texture.destroy(device, allocator);
                    if let Some(staging) = s.staging {
                        staging.destroy();
                    }
                }
            }
            return Err(e);
        }

        // Install textures + write real descriptors. Replaces the
        // fallback redirect installed at enqueue time.
        let staged_count = staged.len();
        let current_frame_id = self.current_frame_id;
        let mut floor_sources = Vec::new();
        let mut landed_changes = Vec::new();
        for s in staged {
            let StagedUpload {
                handle,
                texture,
                staging,
                change,
                floor,
            } = s;

            // Write descriptor for the real image view + sampler.
            let image_view = texture.image_view;
            let sampler = texture.sampler;
            let binding = texture.view_kind.descriptor_binding();
            let pin_as_cube_fallback =
                texture.view_kind == TextureViewKind::Cube && self.cube_fallback_handle.is_none();
            if pin_as_cube_fallback {
                self.cube_fallback_handle = Some(handle);
                self.apply_descriptor_write(device, 0, binding, image_view, sampler);
            }
            self.apply_descriptor_write(device, handle, binding, image_view, sampler);

            // Move the texture into its reserved slot. A residency change
            // replaces the live image, which retires through
            // `pending_destroy` like any replaced image (TXS-RES-004).
            if let Some(entry) = self.textures.get_mut(handle as usize) {
                if let Some(change) = change {
                    if let Some(residency) = entry.residency.as_mut() {
                        residency.apply(change);
                    }
                    landed_changes.push((handle, change));
                    if let Some(old) = entry.texture.replace(texture) {
                        entry.pending_destroy.push_back((current_frame_id, old));
                    }
                } else {
                    entry.texture = Some(texture);
                    if let Some((residency, whole, source, header_len)) = floor {
                        entry.residency = Some(residency);
                        floor_sources.push((handle, whole, source, header_len));
                    }
                }
                if pin_as_cube_fallback {
                    entry.ref_count = u32::MAX;
                }
            } else {
                log::warn!(
                    "flush_pending_uploads: handle {handle} out of bounds; dropping texture"
                );
            }

            // Release staging back to the pool (or destroy if no
            // pool). The fence-wait above guarantees the GPU is done.
            if let Some(staging) = staging {
                if let Some(pool) = self.staging_pool.as_mut() {
                    staging.release_to(pool);
                } else {
                    staging.destroy();
                }
            }
        }

        for (handle, change) in landed_changes {
            self.note_change_landed(handle, change);
        }

        // M39 — every floor records its source; from then on the wanted-mip
        // pass (`apply_wanted`) decides its promotions, read back by the
        // host. No high-mip bytes are kept (decision Q2 (b)).
        for (handle, whole, source, header_len) in floor_sources {
            self.note_floor_install(whole);
            let upload = &pending[source];
            let header = upload.dds_bytes[..header_len.min(upload.dds_bytes.len())].to_vec();
            let width = crate::vulkan::dds::parse_dds(&header)
                .map_or(0, |meta| meta.width.max(meta.height));
            self.stream_sources.insert(
                handle,
                StreamSource {
                    path: upload.path.as_str().into(),
                    clamp_mode: upload.clamp_mode,
                    color_space: upload.color_space,
                    header,
                    width,
                    wanted: None,
                    last_wanted_frame: current_frame_id,
                    last_promoted_frame: None,
                    last_demoted_frame: None,
                    coarser_since: None,
                },
            );
        }

        log::debug!("Flushed DDS upload sub-batch: {staged_count} of {count} staged");
        Ok(staged_count)
    }
}

/// How many queued promotions one flush takes: in queue order while their
/// staged bytes fit `max_bytes`, and always the first (it cannot be split).
pub(super) fn promotion_take_count(
    pending: &[PendingDdsUpload],
    max_bytes: vk::DeviceSize,
) -> usize {
    let mut bytes: vk::DeviceSize = 0;
    let mut take = 0;
    for upload in pending {
        if take > 0 && bytes.saturating_add(upload.staged_bytes) > max_bytes {
            break;
        }
        bytes = bytes.saturating_add(upload.staged_bytes);
        take += 1;
    }
    take
}

/// Bytes each queued upload stages, in queue order — the sizes
/// [`upload_batch_ranges`] budgets. The one place the queue is priced for
/// batching, shared by the flush and by [`TextureRegistry::pending_dds_upload_bytes`]
/// (#4835).
pub(super) fn queued_upload_sizes(pending: &[PendingDdsUpload]) -> Vec<vk::DeviceSize> {
    pending.iter().map(|upload| upload.staged_bytes).collect()
}

/// #4197 — split queued uploads, in queue order, into contiguous sub-batches
/// whose byte sizes sum to at most `budget`.
///
/// An upload larger than `budget` on its own gets a sub-batch of one: a single
/// image cannot be split across submits here, and refusing it would turn a
/// memory bound into a missing texture. Every returned range is non-empty and
/// together they cover `0..sizes.len()` exactly, in order.
pub(super) fn upload_batch_ranges(
    sizes: &[vk::DeviceSize],
    budget: vk::DeviceSize,
) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut bytes: vk::DeviceSize = 0;
    for (index, &size) in sizes.iter().enumerate() {
        if index > start && bytes.saturating_add(size) > budget {
            ranges.push(start..index);
            start = index;
            bytes = 0;
        }
        bytes = bytes.saturating_add(size);
    }
    if start < sizes.len() {
        ranges.push(start..sizes.len());
    }
    ranges
}
