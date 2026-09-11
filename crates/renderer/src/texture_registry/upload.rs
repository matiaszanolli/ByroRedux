//! Upload: the synchronous `load_dds*` path, the deferred `enqueue_dds*`
//! queue, and the per-frame `flush_pending_uploads` drain that turns
//! queued DDS bytes into resident bindless slots.
//!
//! `load_dds*` lives here rather than with lookup because it is the
//! synchronous twin of `enqueue_dds*` — same decode, same slot
//! allocation, a different point in the frame.

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
            has_alpha: crate::vulkan::dds::format_has_alpha(meta.format),
            avg_rgb: None,
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
            // Populated by `flush_pending_uploads` once the DDS header is
            // parsed; `false`/`None` until then, matching the pre-#3682
            // HashMap's "absent handle reads false/None" contract.
            has_alpha: false,
            avg_rgb: None,
        });
        self.path_map.insert(normalized, handle);
        self.pending_dds_uploads.push(PendingDdsUpload {
            handle,
            dds_bytes,
            clamp_mode,
            view_kind,
            color_space,
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
    /// recording error the queue is taken and dropped, not preserved
    /// for retry — every entry in the failed batch is gone, and any
    /// handle already reserved for it (via `queue_or_hit`) stays
    /// `texture: None` forever, cache-HIT-redirected to a dead handle
    /// until every reference to it drops (see #1922 for the fix-vs-
    /// document tradeoff). The partial command buffer is freed
    /// without submit. On submit/fence error the staging buffers leak
    /// into the pool (the GPU may still be reading them) — a
    /// future-proof alternative would defer-destroy them, but
    /// cell-load failure is already a fatal-style error path.
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
        // error the taken `pending` is simply dropped below — there
        // is no push-back, so any entries not yet staged at the point
        // of failure are lost, not retried (#1922).
        let pending = std::mem::take(&mut self.pending_dds_uploads);
        let count = pending.len();

        // Per-upload outputs assembled during recording; consumed
        // after the submit + wait completes.
        struct StagedUpload {
            handle: TextureHandle,
            texture: crate::vulkan::texture::Texture,
            staging: crate::vulkan::buffer::StagingGuard,
            staging_capacity: vk::DeviceSize,
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
                for upload in &pending {
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
                    // #1542: a 16/24-bpp `DDPF_RGB` source is CPU-expanded to
                    // R8G8B8A8. Its raw bytes aren't RGBA8 yet, so skip
                    // `average_rgb` for it (it would misread the packed
                    // pixels) — these are UI/font atlases, never diffuse
                    // albedo anyway.
                    if upload.color_space == TextureColorSpace::Srgb && meta.expand.is_none() {
                        if let Some(avg) = crate::vulkan::dds::average_rgb(&meta, &upload.dds_bytes)
                        {
                            if let Some(entry) = self.textures.get_mut(upload.handle as usize) {
                                entry.avg_rgb = Some(avg);
                            }
                        }
                    }
                    let pixel_data = crate::vulkan::dds::upload_pixels(&meta, &upload.dds_bytes);
                    let sampler = self.samplers[upload.clamp_mode as usize];
                    let (texture, staging, staging_capacity) =
                        match crate::vulkan::texture::Texture::record_dds_upload(
                            device,
                            allocator,
                            cmd,
                            &meta,
                            pixel_data.as_ref(),
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
            // queue is gone (we `take`d it) — but `path_map` was
            // already populated by `queue_or_hit` at enqueue time, so
            // a later request for the same path cache-HITs the dead
            // `texture: None` handle instead of re-queuing (#1922).
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
            let binding = texture.view_kind.descriptor_binding();
            let pin_as_cube_fallback =
                texture.view_kind == TextureViewKind::Cube && self.cube_fallback_handle.is_none();
            if pin_as_cube_fallback {
                self.cube_fallback_handle = Some(handle);
                self.apply_descriptor_write(device, 0, binding, image_view, sampler);
            }
            self.apply_descriptor_write(device, handle, binding, image_view, sampler);

            // Move the texture into its reserved slot.
            if let Some(entry) = self.textures.get_mut(handle as usize) {
                entry.texture = Some(texture);
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
}
