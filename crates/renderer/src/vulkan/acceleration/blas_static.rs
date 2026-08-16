//! Static (mesh-keyed) BLAS lifecycle and builds.
//!
//! Covers the BLAS path that lives in [`super::AccelerationManager::blas_entries`]:
//! single-mesh + batched builds, deferred destroy, eviction. Skinned
//! (per-entity) BLAS live in [`super::blas_skinned`].

use super::super::allocator::SharedAllocator;
use super::super::buffer::GpuBuffer;
use super::super::descriptors::memory_barrier;
use super::super::sync::MAX_FRAMES_IN_FLIGHT;
use super::constants::{BATCH_EVICTION_CHECK_INTERVAL, STATIC_BLAS_FLAGS};
use super::predicates::{
    align_scratch_address, blas_over_budget, scratch_alignment_padding, scratch_needs_growth,
    should_evict_mid_batch, submit_one_time,
};
use super::types::{BlasBuildSource, BlasEntry};
use super::AccelerationManager;
use crate::deferred_destroy::DEFAULT_COUNTDOWN;
use crate::vertex::Vertex;
use anyhow::{Context, Result};
use ash::vk;

/// One compacted static-BLAS entry produced by the compact pass:
/// `(mesh_handle, compacted accel struct, compacted buffer, compacted size,
/// vertex count, index count)`.
type CompactedBlas = (
    u32,
    vk::AccelerationStructureKHR,
    GpuBuffer,
    vk::DeviceSize,
    u32,
    u32,
);

impl AccelerationManager {
    /// Queue a BLAS for deferred destruction.
    ///
    /// Called by the cell loader on unload, where the entry may still be
    /// referenced by an in-flight frame. The entry moves to
    /// `pending_destroy_blas` and the actual `VkAccelerationStructureKHR`
    /// and buffer destruction is delayed until the countdown expires in
    /// [`tick_deferred_destroy`](Self::tick_deferred_destroy).
    /// [`evict_unused_blas`](Self::evict_unused_blas) (the LRU budget path)
    /// uses the same deferred queue, so both load- and unload-path BLAS frees
    /// are safe against in-flight frames (#1449).
    ///
    /// Also forces a full TLAS rebuild on both frame slots so no
    /// subsequent `BUILD`/`UPDATE` references the dropped BLAS address.
    /// See #372. No-op if the handle is not a live BLAS.
    pub fn drop_blas(&mut self, handle: u32) {
        let idx = handle as usize;
        let Some(slot) = self.blas_entries.get_mut(idx) else {
            return;
        };
        let Some(entry) = slot.take() else {
            return;
        };
        self.total_blas_bytes = self.total_blas_bytes.saturating_sub(entry.size_bytes);
        self.static_blas_bytes = self.static_blas_bytes.saturating_sub(entry.size_bytes);
        self.pending_destroy_blas.push(entry, DEFAULT_COUNTDOWN);
        // BLAS map mutated — bump generation so the next build_tlas
        // can short-circuit the per-instance zip-compare. #300.
        self.blas_map_generation = self.blas_map_generation.wrapping_add(1);
        for ref mut t in self.tlas.iter_mut().flatten() {
            t.needs_full_rebuild = true;
        }
    }

    /// Whether `handle` currently owns a live static BLAS.
    pub fn has_blas(&self, handle: u32) -> bool {
        self.blas_entries
            .get(handle as usize)
            .is_some_and(Option::is_some)
    }

    /// Protect the currently eligible rigid draw set from LRU eviction.
    ///
    /// The pre-TLAS recovery pass calls this before it builds any missing
    /// static BLAS. `build_blas_batched` may run budget eviction internally,
    /// so stamping every already-resident draw first prevents that build from
    /// evicting a different mesh which is needed by the same upcoming TLAS.
    /// Missing handles are harmless here; the builder registers them with the
    /// current frame stamp later in the same pass.
    pub fn mark_static_blas_used(&mut self, handles: &[u32]) {
        for &handle in handles {
            if let Some(Some(entry)) = self.blas_entries.get_mut(handle as usize) {
                entry.last_used_frame = self.frame_counter;
            }
        }
    }

    /// Drain and destroy BLAS entries whose defer countdown has reached
    /// zero, and retired `blas_scratch_buffer` allocations
    /// (`pending_destroy_scratch`, #1782) alongside them. Call once per
    /// frame alongside `MeshRegistry::tick_deferred_destroy`.
    pub fn tick_deferred_destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        // Split borrow so the closure can capture `&accel_loader`
        // while the tick borrows `&mut pending_destroy_blas`.
        let Self {
            accel_loader,
            pending_destroy_blas,
            ..
        } = self;
        pending_destroy_blas.tick(|mut entry| {
            // SAFETY: the countdown guarantees no in-flight command
            // buffer still references this acceleration structure.
            unsafe {
                accel_loader.destroy_acceleration_structure(entry.accel, None);
            }
            entry.buffer.destroy(device, allocator);
        });
        self.pending_destroy_scratch.tick(|mut buf| {
            buf.destroy(device, allocator);
        });
    }

    /// Drain `pending_destroy_blas` synchronously, regardless of the
    /// per-entry countdown. Call from a shutdown sweep AFTER
    /// `device_wait_idle` has settled all in-flight command buffers
    /// (the countdown's only purpose is to stand in for that wait).
    /// Each drained entry's BLAS, backing buffer, and `Arc<Mutex<…>>`
    /// allocator clones are released here, ahead of the parent
    /// `VulkanContext::Drop` that would otherwise run the same drain
    /// inline. Counterpart of [`Self::tick_deferred_destroy`] for the
    /// "no future frames will tick the countdown" shutdown path. See
    /// #732 / LIFE-H2.
    ///
    /// # Safety
    ///
    /// Caller must guarantee no live command buffer references any
    /// queued BLAS — typically by an immediately preceding
    /// `device_wait_idle`.
    pub unsafe fn drain_pending_destroys(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
    ) {
        let Self {
            accel_loader,
            pending_destroy_blas,
            ..
        } = self;
        pending_destroy_blas.drain(|mut entry| {
            // SAFETY: the caller's preceding `device_wait_idle` (the drain's
            // `# Safety` precondition) guarantees no in-flight command buffer
            // still references this acceleration structure — standing in for
            // the per-entry countdown that the tick path relies on.
            unsafe {
                accel_loader.destroy_acceleration_structure(entry.accel, None);
            }
            entry.buffer.destroy(device, allocator);
        });
        // SAFETY: same precondition as above — the caller's preceding
        // `device_wait_idle` covers any in-flight command buffer that
        // captured a retired scratch buffer's device address (#1782).
        self.pending_destroy_scratch.drain(|mut buf| {
            buf.destroy(device, allocator);
        });
    }

    /// Number of entries currently waiting in `pending_destroy_blas`.
    /// Surfaced for [`drain_pending_destroys`]'s unit test and shutdown
    /// telemetry — the count must reach zero after a drain. See #732.
    pub fn pending_destroy_blas_count(&self) -> usize {
        self.pending_destroy_blas.len()
    }

    /// Number of retired scratch buffers currently waiting in
    /// `pending_destroy_scratch`. Surfaced for the deferred-destroy
    /// regression test and shutdown telemetry — the count must reach
    /// zero after a drain. See #1782.
    pub fn pending_destroy_scratch_count(&self) -> usize {
        self.pending_destroy_scratch.len()
    }

    /// Build BLAS for multiple meshes in a single command buffer submission.
    ///
    /// This eliminates the per-mesh fence stall from `build_blas` by recording
    /// all BLAS build commands into one command buffer with memory barriers
    /// between builds that share the scratch buffer. For 3000 meshes, this
    /// reduces scene load from 150-600ms (3000 fence round-trips) to ~5-15ms
    /// (single submission + one fence wait).
    ///
    /// Each build reuses the shared `blas_scratch_buffer` (grown to the max
    /// scratch size needed). Builds are serialized within the command buffer
    /// via `ACCELERATION_STRUCTURE_BUILD` → `ACCELERATION_STRUCTURE_BUILD`
    /// memory barriers since they share scratch memory.
    pub fn build_blas_batched(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        transfer_fence: Option<&std::sync::Mutex<vk::Fence>>,
        meshes: &[BlasBuildSource],
    ) -> Result<usize> {
        if meshes.is_empty() {
            return Ok(0);
        }

        // Advance frame_counter so evict_unused_blas sees meaningful idle
        // counts during cell-streaming bursts. build_tlas also bumps it
        // once per draw_frame, but draw_frame never runs between back-to-back
        // build_blas_batched calls during initial cell loads (M40 streaming).
        // Without this bump, every entry looks idle=0 and the BLAS budget
        // is unenforced across loading bursts.
        //
        // #1793 / PERF-D3-NEW-02 — this same per-call bump is also the
        // cause of a DIFFERENT bug: a synchronous multi-cell burst (e.g.
        // `--grid` radius 3 = 49 calls before the first real frame) means
        // cell #1's entries are stamped with an early `frame_counter`
        // value, then cell #49's call bumps the SAME shared counter 48
        // more times before the first post-burst `build_tlas` measures
        // idle-ness — aging cell #1's brand-new, not-yet-drawn entries by
        // 48 "ticks" that represent OTHER cells loading, not real elapsed
        // frames. `build_tlas` does re-stamp `last_used_frame` for any
        // entry actually referenced by a `DrawCommand` that frame, but a
        // streaming look-ahead BLAS with no draw command yet (built for a
        // cell that isn't in view but was eagerly pre-built) has nothing
        // to protect it and can become a false LRU-eviction victim before
        // it's ever drawn once.
        //
        // Not fixed here: doing so correctly needs a burst-boundary signal
        // from the caller (there's no single call site — exterior grid
        // loads, interior loads, `scene/nif_loader.rs`, and `cornell.rs`
        // each independently loop over `build_blas_batched` calls) so a
        // "just-loaded, not-yet-real-frame-measured" entry can be told
        // apart from a genuinely-idle one, without weakening the
        // "unenforced across loading bursts" property this same bump
        // exists for (a naive removal of the per-call bump reintroduces
        // THAT bug). Deferred pending a `--grid` + low-VRAM-budget repro
        // to validate a fix against — this is CPU bookkeeping only (no
        // crash risk), but the failure mode (false eviction of a mesh
        // that was never actually stale) has bitten this exact subsystem
        // via subtle counter-semantics bugs before (#920, #1449).
        self.frame_counter = self.frame_counter.wrapping_add(1);

        let vertex_stride = std::mem::size_of::<Vertex>() as vk::DeviceSize;

        // Phase 1: Query sizes and allocate result buffers for all meshes.
        struct PreparedBlas {
            mesh_handle: u32,
            accel: vk::AccelerationStructureKHR,
            buffer: GpuBuffer,
            /// `vk::AccelerationStructureGeometryKHR<'a>` carries a
            /// PHANTOM lifetime from ash's typed builder API — the
            /// compiler can't see that every union field used in the
            /// `BLAS-from-device-buffer` path is value-typed (`u64`
            /// device addresses + small enums), so without an
            /// annotation the borrow checker would tie the struct's
            /// lifetime to the local `triangles_data` Vec. We fill
            /// only `device_address: u64` (no host pointers, no Rust
            /// references) so the `'static` claim is sound.
            ///
            /// **Future-proof invariant**: every `.geometry()`-reachable
            /// field must remain value-typed. Adding a host-pointer
            /// variant or a `&[T]` body would make this UB with no
            /// compiler warning. See #580 / SAFE-21.
            geometry: vk::AccelerationStructureGeometryKHR<'static>,
            primitive_count: u32,
            /// Per-mesh scratch size from Phase 1 sizing — stored so the
            /// final `BlasEntry` can remember it for
            /// `shrink_blas_scratch_to_fit` (#495). Max across meshes is
            /// tracked separately in `max_scratch_size` for the single
            /// shared build scratch allocation.
            build_scratch_size: vk::DeviceSize,
            /// #907 — counts captured here so the final `BlasEntry`
            /// can pin them for the refit-counts VUID check. Static
            /// BLAS never refit so these are read-only telemetry on
            /// the resulting entry; included for symmetry with the
            /// skinned path that DOES validate against them.
            vertex_count: u32,
            index_count: u32,
        }

        let mut prepared: Vec<PreparedBlas> = Vec::with_capacity(meshes.len());
        let mut max_scratch_size: vk::DeviceSize = 0;

        // We need to keep the triangles data alive for the geometry references.
        // Store them in a parallel vec since the geometry structs reference them.
        let mut triangles_data: Vec<vk::AccelerationStructureGeometryTrianglesDataKHR> =
            Vec::with_capacity(meshes.len());

        for source in meshes {
            // SAFETY: the per-mesh vertex buffer was created with
            // SHADER_DEVICE_ADDRESS; the returned address is valid for the
            // buffer's lifetime.
            let vertex_address = unsafe {
                device.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default().buffer(source.vertex_buffer),
                )
            } + source.vertex_byte_offset;
            // SAFETY: the per-mesh index buffer was created with
            // SHADER_DEVICE_ADDRESS; the returned address is valid for the
            // buffer's lifetime.
            let index_address = unsafe {
                device.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default().buffer(source.index_buffer),
                )
            } + source.index_byte_offset;

            let triangles = vk::AccelerationStructureGeometryTrianglesDataKHR::default()
                .vertex_format(vk::Format::R32G32B32_SFLOAT)
                .vertex_data(vk::DeviceOrHostAddressConstKHR {
                    device_address: vertex_address,
                })
                .vertex_stride(vertex_stride)
                .max_vertex(source.vertex_count.saturating_sub(1))
                .index_type(vk::IndexType::UINT32)
                .index_data(vk::DeviceOrHostAddressConstKHR {
                    device_address: index_address,
                });

            triangles_data.push(triangles);
        }

        // Pre-batch eviction — release any previous-cell BLAS that is
        // safely past `MAX_FRAMES_IN_FLIGHT + 1` idle before we start
        // creating result buffers. Cheap when nothing qualifies
        // (`evict_unused_blas` early-returns under budget); helps cell
        // transitions where the outgoing cell's BLAS still holds live
        // memory that the incoming cell's batch is about to need. #510.
        // #2692 — as at the single-shot site above: eviction is deferred
        // through `pending_destroy_blas` + `DEFAULT_COUNTDOWN`, which is the
        // real cross-frame guarantee; the idle threshold this comment used to
        // cite is LRU policy only (#1449). The prepared buffers for this batch
        // are additionally not yet in `blas_entries`, so they cannot be
        // candidates at all.
        //
        // #1792 — `pending_bytes = 0`: nothing in this batch has been
        // sized yet at this point (the loop below hasn't run).
        {
            self.evict_unused_blas(device, allocator, 0);
        }

        // Running sum of `acceleration_structure_size` across the Phase 1
        // buffers we've created for *this batch* (all static BLAS — this
        // codepath is the static / mesh-keyed builder). Combined with
        // `self.static_blas_bytes` it gives the projected static footprint
        // the mid-batch eviction predicate tests. See
        // [`should_evict_mid_batch`]. The compare uses
        // `static_blas_bytes` not `total_blas_bytes` so skinned-BLAS
        // residency on NPC-heavy scenes can't trigger eviction of static
        // BLAS that the budget can't actually free (#920).
        //
        // #2927 / PERF-D3-03 — this ledger deliberately covers Phase 1
        // ONLY (the uncompacted originals). It is NOT the batch peak: the
        // compaction phase allocates a second, compacted set alongside
        // these while every original is still live, so real peak residency
        // is `static_blas_bytes + pending_bytes + total_after`. That larger
        // figure is checked once, with exact sizes, at the head of
        // `alloc_compact` below — see the `evict_unused_blas` call there.
        // Keep the two in sync: a future budget tune made against
        // `pending_bytes` alone is being made against ~⅔ of the real number.
        let mut pending_bytes: vk::DeviceSize = 0;
        // Now build geometries referencing the stored triangles data.
        for (idx, source) in meshes.iter().enumerate() {
            let mesh_handle = source.mesh_handle;
            let vertex_count = source.vertex_count;
            let index_count = source.index_count;
            // Mid-batch eviction check. Trigger only every N iterations
            // so the cost is amortized; the predicate itself is pure
            // arithmetic. #510.
            if idx > 0
                && idx % BATCH_EVICTION_CHECK_INTERVAL == 0
                && should_evict_mid_batch(
                    self.static_blas_bytes,
                    pending_bytes,
                    self.blas_budget_bytes,
                )
            {
                // SAFETY: prepared buffers for this batch are local
                // to `prepared` and not yet in `self.blas_entries`,
                // so `evict_unused_blas` cannot touch them — it only
                // frees entries in `blas_entries` that are past the
                // idle threshold.
                //
                // #1792 / PERF-D3-NEW-01 — pass the real `pending_bytes`
                // accumulated so far this batch. Before this fix the
                // callee's own budget gate only ever saw the pre-batch
                // committed total (`static_blas_bytes`), so on a fresh
                // load (`static_blas_bytes == 0`) it early-returned and
                // evicted nothing no matter how large this batch's
                // already-allocated result buffers had grown — the
                // trigger above fired, but the callee it called was
                // structurally blind to the very bytes that triggered it.
                self.evict_unused_blas(device, allocator, pending_bytes);
            }

            let primitive_count = index_count / 3;

            // SAFETY: `vk::AccelerationStructureGeometryKHR<'a>` carries a
            // phantom lifetime from ash's typed builder API. We never
            // populate a Rust borrow into the geometry union — every
            // field in `vk::AccelerationStructureGeometryDataKHR.triangles`
            // we reach (vertex / index `device_address: u64`,
            // `vertex_format`, `index_type`, primitive count, etc.) is
            // value-typed; no host pointers, no `&[T]`. The `'static`
            // annotation on `PreparedBlas::geometry` (line ~610) is
            // therefore sound regardless of whether `triangles_data`
            // is still in scope.
            //
            // The geometry value itself is consumed inline below to
            // build the per-BLAS sizes query and stored on
            // `PreparedBlas::geometry` for the Phase 2 batched build
            // submission. No real cross-Vec borrow lives across that
            // boundary.
            //
            // Pre-#580 this comment claimed both "triangles_data lives
            // for the function" (which would imply a real borrow) and
            // "geometry holds a copy of the union data" (correct);
            // only the second half is the real invariant. See SAFE-21.
            let geometry = vk::AccelerationStructureGeometryKHR::default()
                .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
                .flags(vk::GeometryFlagsKHR::OPAQUE)
                .geometry(vk::AccelerationStructureGeometryDataKHR {
                    triangles: triangles_data[idx],
                });

            let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                .flags(STATIC_BLAS_FLAGS)
                .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                .geometries(std::slice::from_ref(&geometry));

            let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
            // SAFETY: query-only call; `accel_loader`, `build_info`
            // (value-typed geometry) and `sizes` out-param are live; device
            // outlives it.
            unsafe {
                self.accel_loader.get_acceleration_structure_build_sizes(
                    vk::AccelerationStructureBuildTypeKHR::DEVICE,
                    &build_info,
                    &[primitive_count],
                    &mut sizes,
                );
            };

            max_scratch_size = max_scratch_size.max(sizes.build_scratch_size);
            pending_bytes = pending_bytes.saturating_add(sizes.acceleration_structure_size);

            let mut result_buffer = GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                sizes.acceleration_structure_size,
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            )?;

            let accel_info = vk::AccelerationStructureCreateInfoKHR::default()
                .buffer(result_buffer.buffer)
                .size(sizes.acceleration_structure_size)
                .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL);

            // SAFETY: `accel_info` references `result_buffer.buffer`, just
            // created with ACCELERATION_STRUCTURE_STORAGE_KHR and still live;
            // device outlives the call. On failure the already-prepared
            // entries (owned by `prepared`, no command buffer yet references
            // them) are destroyed before bailing.
            let accel = unsafe {
                match self
                    .accel_loader
                    .create_acceleration_structure(&accel_info, None)
                {
                    Ok(a) => a,
                    Err(e) => {
                        // #1097 / REN-D8-003 — clean up previously-prepared
                        // entries before bailing. Pre-fix, only the current
                        // iteration's `result_buffer` was destroyed; entries
                        // already in `prepared[0..i-1]` leaked their
                        // GpuBuffer + VkAccelerationStructureKHR handles.
                        result_buffer.destroy(device, allocator);
                        for mut p in prepared {
                            // SAFETY: each entry's accel + buffer are owned
                            // by `prepared` (just moved in by push); no
                            // command buffer references them yet (the build
                            // hasn't been recorded).
                            self.accel_loader
                                .destroy_acceleration_structure(p.accel, None);
                            p.buffer.destroy(device, allocator);
                        }
                        anyhow::bail!("Failed to create BLAS for mesh {mesh_handle}: {e}");
                    }
                }
            };

            prepared.push(PreparedBlas {
                mesh_handle,
                accel,
                buffer: result_buffer,
                geometry,
                primitive_count,
                build_scratch_size: sizes.build_scratch_size,
                vertex_count,
                index_count,
            });
        }

        // Phase 2: Ensure scratch buffer is large enough. Grow-only
        // policy via shared helper — see #60 / #424 SIBLING. Pad by
        // `scratch_alignment_padding` so the shared device address can be
        // rounded up to `scratch_align` below (#1386).
        let scratch_size = max_scratch_size + scratch_alignment_padding(self.scratch_align);
        let need_new_scratch = scratch_needs_growth(
            self.blas_scratch_buffer.as_ref().map(|b| b.size),
            scratch_size,
        );

        if need_new_scratch {
            // #1782 — see the matching comment in `build_blas` above.
            // This is the M40 streaming hot path (called from
            // `step_streaming` in `about_to_wait`), the exact window
            // where the previously-submitted frame's skinned-BLAS
            // refit/first-sight command buffer may still be executing
            // on the GPU and referencing `old`'s scratch device
            // address. Deferred-destroy, not immediate.
            if let Some(old) = self.blas_scratch_buffer.take() {
                self.pending_destroy_scratch.push(old, DEFAULT_COUNTDOWN);
            }
            self.blas_scratch_buffer = Some(GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                scratch_size,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            )?);
        }

        // Round the raw device address up to `scratch_align` so the
        // address shared by every recorded build in this batch satisfies
        // VUID-…-pInfos-03715 in release too (the headroom above absorbs
        // the shift). No-op on aligned drivers. See #1386 / #659.
        // SAFETY: the shared scratch buffer was created with
        // SHADER_DEVICE_ADDRESS; the returned address is rounded up to
        // `scratch_align` below into the padding reserved above.
        let raw_scratch = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default()
                    .buffer(self.blas_scratch_buffer.as_ref().unwrap().buffer),
            )
        };
        let scratch_address = align_scratch_address(raw_scratch, self.scratch_align);

        // Phase 3: Create query pool for compacted size readback.
        let n = prepared.len() as u32;
        let query_pool_info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::ACCELERATION_STRUCTURE_COMPACTED_SIZE_KHR)
            .query_count(n);
        // SAFETY: `query_pool_info` is fully initialized and device is
        // live; the returned pool is owned and destroyed below.
        let query_pool = unsafe {
            device
                .create_query_pool(&query_pool_info, None)
                .context("Failed to create compaction query pool")?
        };
        // Reset the query pool before use (required by Vulkan spec).
        // SAFETY: `query_pool` was just created with `n` queries; the
        // reset range [0, n) is in bounds; no query is in use yet.
        unsafe {
            device.reset_query_pool(query_pool, 0, n);
        }

        // Phase 4: Record builds + compaction size queries into one command buffer.
        let build_result = submit_one_time(device, queue, command_pool, transfer_fence, |cmd| {
            for (i, p) in prepared.iter().enumerate() {
                if i > 0 {
                    self.record_scratch_serialize_barrier(device, cmd);
                }

                let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                    .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                    .flags(STATIC_BLAS_FLAGS)
                    .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                    .dst_acceleration_structure(p.accel)
                    .geometries(std::slice::from_ref(&p.geometry))
                    .scratch_data(vk::DeviceOrHostAddressKHR {
                        device_address: scratch_address,
                    });

                let range_info = vk::AccelerationStructureBuildRangeInfoKHR::default()
                    .primitive_count(p.primitive_count)
                    .primitive_offset(0)
                    .first_vertex(0);

                // SAFETY: `cmd` is recording (inside `submit_one_time`); the
                // shared scratch is sized to `max_scratch_size` + alignment
                // padding and the per-build scratch ranges are serialized by the
                // barrier at the loop head; `p.accel` is freshly created and not
                // referenced by any other in-flight build; geometry handles live.
                unsafe {
                    self.accel_loader.cmd_build_acceleration_structures(
                        cmd,
                        &[build_info],
                        &[std::slice::from_ref(&range_info)],
                    );
                }
            }

            // Barrier: all builds must complete before querying compacted sizes.
            // AS_BUILD_KHR → AS_BUILD_KHR (WRITE → READ for compaction query).
            // SAFETY: `cmd` is recording; the barrier serializes all preceding
            // AS builds (WRITE) against the compaction-size queries (READ) that
            // follow on the same command buffer.
            unsafe {
                memory_barrier(
                    device,
                    cmd,
                    vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                    vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
                    vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                    vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR,
                );
            }

            // Query compacted sizes for all built BLAS.
            let accel_handles: Vec<vk::AccelerationStructureKHR> =
                prepared.iter().map(|p| p.accel).collect();
            // SAFETY: `cmd` is recording; every `accel` in `accel_handles` was
            // built earlier on this command buffer and the barrier above orders
            // the build writes before this read; `query_pool` holds `n` slots.
            unsafe {
                self.accel_loader
                    .cmd_write_acceleration_structures_properties(
                        cmd,
                        &accel_handles,
                        vk::QueryType::ACCELERATION_STRUCTURE_COMPACTED_SIZE_KHR,
                        query_pool,
                        0,
                    );
            }

            Ok(())
        });

        if let Err(e) = build_result {
            for mut p in prepared {
                // SAFETY: the build submission failed, so no in-flight command
                // buffer references `p.accel`; each accel + buffer is owned by
                // `prepared`; device is live.
                unsafe {
                    self.accel_loader
                        .destroy_acceleration_structure(p.accel, None);
                }
                p.buffer.destroy(device, allocator);
            }
            // SAFETY: `query_pool` is the live pool created above; device is
            // live; no in-flight command buffer references it after the failed
            // submit.
            unsafe {
                device.destroy_query_pool(query_pool, None);
            }
            return Err(e);
        }

        // Phases 5 + 6: Read back compacted sizes, then allocate compacted
        // destination buffers + acceleration structures. Wrapped in a
        // closure so that any mid-loop allocation failure can roll back
        // the partial compact-side state plus the still-owned `prepared`
        // originals and the `query_pool`. Pre-#316 these `?` exits leaked
        // every Vulkan handle whose Drop relies on the explicit `destroy`
        // calls in phase 7. Mirrors the build/copy-phase cleanup pattern
        // at lines 733-745 / 815-832.
        //
        // #2926 / PERF-D3-02 — `compact_accels` is owned by the CALLER of
        // the closure and passed in by `&mut`, NOT declared inside it.
        // #316's rollback reached `prepared` + the query pool only: every
        // compacted `vk::AccelerationStructureKHR` an earlier iteration had
        // already pushed was dropped as plain memory on both of the
        // closure's early exits (the `create_device_local_uninit` `?` and
        // the `create_acceleration_structure` `bail!`). `GpuBuffer` has a
        // `Drop` safety net (#656) that reclaims the buffer; the raw
        // acceleration-structure handle has no `Drop` impl at all and
        // leaked for the process lifetime — the same reasoning as #2481,
        // on the one path (allocator OOM) where leaking makes the *next*
        // attempt fail sooner. Owning the vec outside means whatever the
        // closure managed to allocate survives the error and is visible to
        // the rollback arm below.
        let mut compact_accels: Vec<CompactedBlas> = Vec::with_capacity(prepared.len());
        let mut alloc_compact = |compact_accels: &mut Vec<CompactedBlas>| -> Result<(u64, u64)> {
            let mut compacted_sizes = vec![0u64; prepared.len()];
            // SAFETY: the WAIT flag blocks until all `n` compaction-size
            // queries written above are available; `compacted_sizes` has one
            // slot per query; device + pool are live.
            unsafe {
                device
                    .get_query_pool_results(
                        query_pool,
                        0,
                        &mut compacted_sizes,
                        vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
                    )
                    .context("Failed to read compaction query results")?;
            }

            let total_before: u64 = prepared.iter().map(|p| p.buffer.size).sum();
            let total_after: u64 = compacted_sizes.iter().sum();

            // #2927 / PERF-D3-03 — the real static-BLAS peak for this
            // batch. The Phase-1 `pending_bytes` ledger (which drove the
            // mid-batch `should_evict_mid_batch` checks) stopped at
            // `total_before`: it never saw the compaction destinations,
            // which are a SECOND full set of buffers allocated below while
            // every Phase-1 original is still live (they aren't destroyed
            // until Phase 7, after the copy submission retires). True peak
            // residency is therefore `static_blas_bytes + total_before +
            // total_after`, ~1.5× what the budget was being tested
            // against, and there was no eviction check anywhere inside
            // this phase — the one that pushes residency to its maximum.
            //
            // Unlike the Phase-1 loop, this needs no interval amortization
            // and no per-iteration re-check: `get_query_pool_results` above
            // has already handed us every compacted size, so the exact peak
            // is known here, before a single destination is allocated. One
            // pre-emptive call with the true figure is both cheaper and
            // strictly more effective than the per-N-iterations sampling
            // #1792 installed upstream. No-op when under budget (the callee
            // early-returns), which is every case on a 12 GB card; this is a
            // 6 GB-RT-minimum-target path, like the rest of `blas_budget_bytes`.
            self.evict_unused_blas(device, allocator, total_before.saturating_add(total_after));

            // Tuple: (mesh_handle, compacted accel, compacted buffer,
            // build_scratch_size, vertex_count, index_count). Scratch
            // size is propagated from `prepared` so the final
            // `BlasEntry` can remember what scratch this mesh consumed
            // at build time (#495); vertex/index counts are propagated
            // for the refit-counts VUID check (#907 — static BLAS
            // never refit but we pin the counts for symmetry).
            for (i, p) in prepared.iter().enumerate() {
                let compact_size = compacted_sizes[i];

                let compact_buffer = GpuBuffer::create_device_local_uninit(
                    device,
                    allocator,
                    compact_size,
                    vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                )?;

                let compact_accel_info = vk::AccelerationStructureCreateInfoKHR::default()
                    .buffer(compact_buffer.buffer)
                    .size(compact_size)
                    .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL);

                // SAFETY: `compact_accel_info` references `compact_buffer.buffer`,
                // just created with ACCELERATION_STRUCTURE_STORAGE_KHR and live;
                // device outlives the call. On failure the local buffer is
                // destroyed before bailing.
                let compact_accel = unsafe {
                    match self
                        .accel_loader
                        .create_acceleration_structure(&compact_accel_info, None)
                    {
                        Ok(a) => a,
                        Err(e) => {
                            // Buffer was created in this iteration but not
                            // yet pushed into `compact_accels`, so the outer
                            // cleanup loop (which since #2926 does exist and
                            // does walk `compact_accels`) won't see it —
                            // destroy it locally before bubbling so the OOM
                            // path is leak-free. Earlier iterations' entries
                            // are already in `compact_accels` and are the
                            // rollback arm's responsibility.
                            let mut b = compact_buffer;
                            b.destroy(device, allocator);
                            anyhow::bail!("Failed to create compact BLAS: {e}");
                        }
                    }
                };

                compact_accels.push((
                    p.mesh_handle,
                    compact_accel,
                    compact_buffer,
                    p.build_scratch_size,
                    p.vertex_count,
                    p.index_count,
                ));
            }

            Ok((total_before, total_after))
        };

        let (total_before, total_after) = match alloc_compact(&mut compact_accels) {
            Ok(v) => v,
            Err(e) => {
                // Roll back: destroy whatever compaction destinations were
                // already allocated (#2926 — the closure's own early exits
                // leave them in `compact_accels`; only the failing
                // iteration's not-yet-pushed buffer is cleaned up inside),
                // then the originals (phase 7's job on the happy path) and
                // the query pool. Mirrors the `copy_result` failure arm
                // below, which has always walked both vecs correctly.
                for (_, accel, mut buf, _, _, _) in compact_accels {
                    // SAFETY: the compaction allocation failed before any copy
                    // was recorded, so no in-flight command buffer references
                    // this compacted `accel`; each accel + buffer is owned by
                    // `compact_accels`; device is live.
                    unsafe {
                        self.accel_loader
                            .destroy_acceleration_structure(accel, None);
                    }
                    buf.destroy(device, allocator);
                }
                for mut p in prepared {
                    // SAFETY: the compaction allocation failed before any copy was
                    // recorded, so no in-flight command buffer references `p.accel`;
                    // each accel + buffer is owned by `prepared`; device is live.
                    unsafe {
                        self.accel_loader
                            .destroy_acceleration_structure(p.accel, None);
                    }
                    p.buffer.destroy(device, allocator);
                }
                // SAFETY: `query_pool` is the live pool created above; device is
                // live; no in-flight command buffer references it on this path.
                unsafe {
                    device.destroy_query_pool(query_pool, None);
                }
                return Err(e);
            }
        };

        // Record compaction copies in a second command buffer.
        let copy_result = submit_one_time(device, queue, command_pool, transfer_fence, |cmd| {
            for (i, (_, compact_accel, _, _, _, _)) in compact_accels.iter().enumerate() {
                let copy_info = vk::CopyAccelerationStructureInfoKHR::default()
                    .src(prepared[i].accel)
                    .dst(*compact_accel)
                    .mode(vk::CopyAccelerationStructureModeKHR::COMPACT);

                // SAFETY: `cmd` is recording; `prepared[i].accel` (src) was built
                // and the compaction barrier ordered its write; `*compact_accel`
                // (dst) was sized from the queried compacted size; no other
                // in-flight build aliases either handle.
                unsafe {
                    self.accel_loader
                        .cmd_copy_acceleration_structure(cmd, &copy_info);
                }
            }
            Ok(())
        });

        // Destroy the query pool — no longer needed.
        // SAFETY: the compaction-size queries have been read back; the
        // pool is no longer referenced by any command buffer; device is
        // live.
        unsafe {
            device.destroy_query_pool(query_pool, None);
        }

        if let Err(e) = copy_result {
            // Clean up both original and compact structures on failure.
            for mut p in prepared {
                // SAFETY: the copy submission failed, so no in-flight command
                // buffer references `p.accel`; each accel + buffer is owned by
                // `prepared`; device is live.
                unsafe {
                    self.accel_loader
                        .destroy_acceleration_structure(p.accel, None);
                }
                p.buffer.destroy(device, allocator);
            }
            for (_, accel, mut buf, _, _, _) in compact_accels {
                // SAFETY: the copy submission failed, so the compacted `accel` was
                // never read by any in-flight command buffer; each accel + buffer
                // is owned by `compact_accels`; device is live.
                unsafe {
                    self.accel_loader
                        .destroy_acceleration_structure(accel, None);
                }
                buf.destroy(device, allocator);
            }
            return Err(e);
        }

        // Phase 7: Destroy originals, store compacted entries.
        for mut p in prepared {
            // SAFETY: the compaction copy completed (the `submit_one_time`
            // fence has retired), so no command buffer still references the
            // original `p.accel`; each accel + buffer is owned by `prepared`;
            // device is live.
            unsafe {
                self.accel_loader
                    .destroy_acceleration_structure(p.accel, None);
            }
            p.buffer.destroy(device, allocator);
        }

        let count = compact_accels.len();
        for (mesh_handle, accel, buffer, build_scratch_size, vertex_count, index_count) in
            compact_accels
        {
            // SAFETY: `accel` is the live compacted BLAS; query-only call;
            // device outlives it.
            let device_address = unsafe {
                self.accel_loader.get_acceleration_structure_device_address(
                    &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                        .acceleration_structure(accel),
                )
            };

            let handle = mesh_handle as usize;
            let blas_size = buffer.size;
            while self.blas_entries.len() <= handle {
                self.blas_entries.push(None);
            }
            // #2481 / AS-D1-NEW-02 — see the matching guard in `build_blas`
            // above: release any BLAS already occupying this handle through
            // the deferred-destroy queue before overwriting it, instead of
            // dropping a live `BlasEntry` (and leaking its raw
            // `vk::AccelerationStructureKHR`) as plain memory.
            self.drop_blas(mesh_handle);
            self.total_blas_bytes += blas_size;
            self.static_blas_bytes += blas_size;
            self.blas_entries[handle] = Some(BlasEntry {
                accel,
                buffer,
                device_address,
                last_used_frame: self.frame_counter,
                size_bytes: blas_size,
                build_scratch_size,
                // Static (mesh-keyed) BLAS never refit. See #679.
                refit_count: 0,
                built_vertex_count: vertex_count,
                built_index_count: index_count,
                // #1145 — record for symmetry / telemetry. Static
                // BLAS never refit so this field is read-only here.
                built_flags: STATIC_BLAS_FLAGS,
            });
        }
        // BLAS map mutated (one bump for the whole batch — generation is
        // a "did anything change" flag, not a count). See #300.
        if count > 0 {
            self.blas_map_generation = self.blas_map_generation.wrapping_add(1);
        }

        let savings_pct = if total_before > 0 {
            100.0 * (1.0 - total_after as f64 / total_before as f64)
        } else {
            0.0
        };
        log::info!(
            "Batched BLAS build: {} meshes, compacted {:.1} KB → {:.1} KB ({:.0}% savings)",
            count,
            total_before as f64 / 1024.0,
            total_after as f64 / 1024.0,
            savings_pct,
        );
        Ok(count)
    }

    /// Evict unused BLAS entries when static BLAS memory (plus any
    /// caller-known `pending_bytes` not yet committed to
    /// `static_blas_bytes`) exceeds the budget.
    ///
    /// Entries unused for more than `min_idle_frames` frames are candidates.
    /// Eviction is LRU — the least recently used entries are reclaimed first.
    ///
    /// The budget compare uses `static_blas_bytes`, NOT `total_blas_bytes`,
    /// because skinned per-entity BLAS aren't eviction candidates (see
    /// `static_blas_bytes` doc on the struct field for details / #920).
    ///
    /// #1792 / PERF-D3-NEW-01 — `pending_bytes` lets a mid-batch caller
    /// (`build_blas_batched`'s per-iteration `should_evict_mid_batch`
    /// check) report the sum of `acceleration_structure_size` already
    /// committed to result buffers *this batch* but not yet folded into
    /// `static_blas_bytes` (that only happens in the batch's Phase 7,
    /// after every result buffer is already allocated). Without it, this
    /// gate and the loop break below only ever saw the committed-before-
    /// this-batch total — on a fresh load (`static_blas_bytes == 0`) a
    /// single oversized batch sailed straight past the budget with zero
    /// intervening eviction, deferred until the *next* cell load. Callers
    /// with no batch context (the per-frame `draw.rs` call, the two
    /// pre-batch calls before any result buffer in this batch has been
    /// sized) pass `0`, preserving prior behavior exactly.
    ///
    /// The budget line here stays the real 100% (`static_blas_bytes +
    /// pending_bytes <= blas_budget_bytes`), NOT `should_evict_mid_batch`'s
    /// 90% early-warning line — that 90% only decides *when to bother
    /// checking* (amortized every `BATCH_EVICTION_CHECK_INTERVAL`
    /// iterations); how much this function actually reclaims is still
    /// governed by the same 100% target the per-frame call already used.
    ///
    /// Like [`drop_blas`](Self::drop_blas), eviction routes the
    /// `VkAccelerationStructureKHR` + backing buffer through
    /// `pending_destroy_blas` (deferred-destroy) rather than freeing them
    /// inline. The per-entry countdown (`MAX_FRAMES_IN_FLIGHT`) is drained in
    /// `tick_deferred_destroy` only after the per-frame fence proves the
    /// referencing frame retired, so eviction is safe even when streaming runs
    /// `build_blas_batched` while frames are in flight (the #1449 device-loss
    /// this replaced). The `min_idle` gate below is now just an LRU heuristic,
    /// no longer a safety mechanism (MEM-01 / #1449; was REN-D8-NEW-16 / #960).
    ///
    /// MEM-01 / #1449 (FIXED): eviction now routes the AS + buffer free through
    /// `pending_destroy_blas` (deferred-destroy), so it is safe even when
    /// `build_blas_batched` runs while frames are in flight. The original
    /// immediate-destroy path assumed `frame_counter` advanced at most once per
    /// *retired* frame; the streaming-during-render path violated that
    /// (`build_blas_batched` bumps `frame_counter` per call during
    /// `step_streaming`, which runs in `about_to_wait` BEFORE the next
    /// `draw_frame`'s fence wait), freeing a BLAS the in-flight previous TLAS
    /// still referenced → `VK_ERROR_DEVICE_LOST`. The deferred countdown
    /// (= `MAX_FRAMES_IN_FLIGHT`) now stands in for the fence wait, exactly as
    /// `drop_blas` / `drop_skinned` already do.
    ///
    /// #2692 — this used to be `pub unsafe fn` with a `# Safety` section, and
    /// the marker was documented in its own body as vestigial. It is now a safe
    /// fn: eviction only moves entries onto `pending_destroy_blas`, and the
    /// actual `vkDestroyAccelerationStructureKHR` happens in
    /// `tick_deferred_destroy`. `device`/`allocator` are still taken so the
    /// call sites keep a stable signature (and so the deferred path can be
    /// re-inlined without another churn), but nothing here dereferences them.
    pub fn evict_unused_blas(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        pending_bytes: vk::DeviceSize,
    ) {
        // The GPU free is deferred (see the loop body), so this function never
        // touches `device`/`allocator` directly — `tick_deferred_destroy` does.
        // The params are retained so the call sites
        // (`build_blas`/`build_blas_batched`/`draw.rs`) keep a stable signature.
        // The vestigial `unsafe` marker this comment used to promise a follow-up
        // for was dropped in #2692.
        let _ = (device, allocator);

        if !blas_over_budget(
            self.static_blas_bytes,
            pending_bytes,
            self.blas_budget_bytes,
        ) {
            return;
        }

        // #1449 / MEM-01 — eviction routes through `pending_destroy_blas`
        // (deferred-destroy), so the idle gate below is now purely an **LRU
        // policy** ("don't evict a BLAS used in the last few frames"), NOT the
        // safety mechanism it used to be. Before the fix, eviction destroyed the
        // AS immediately and relied on `idle >= MIN_IDLE_FRAMES` to stand in for
        // a fence wait — which broke once streaming ran `build_blas_batched`
        // (bumping `frame_counter` per call) while frames were in flight, freeing
        // a BLAS the in-flight TLAS still referenced (→ device loss). The
        // deferred countdown now provides the real cross-frame safety; the gate
        // staying at `MAX_FRAMES_IN_FLIGHT + 1` is just a sensible LRU default.
        const MIN_IDLE_FRAMES: u64 = MAX_FRAMES_IN_FLIGHT as u64 + 1;
        let min_idle = MIN_IDLE_FRAMES;
        let current = self.frame_counter;

        // Collect eviction candidates: (index, last_used_frame, size).
        let mut candidates: Vec<(usize, u64, vk::DeviceSize)> = self
            .blas_entries
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| {
                slot.as_ref().and_then(|blas| {
                    let idle = current.saturating_sub(blas.last_used_frame);
                    if idle >= min_idle {
                        Some((i, blas.last_used_frame, blas.size_bytes))
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Sort by oldest first (LRU).
        candidates.sort_unstable_by_key(|&(_, frame, _)| frame);

        let mut evicted = 0usize;
        let mut freed = 0u64;
        for (idx, _, _size) in candidates {
            if !blas_over_budget(
                self.static_blas_bytes,
                pending_bytes,
                self.blas_budget_bytes,
            ) {
                break;
            }
            if let Some(entry) = self.blas_entries[idx].take() {
                self.total_blas_bytes = self.total_blas_bytes.saturating_sub(entry.size_bytes);
                self.static_blas_bytes = self.static_blas_bytes.saturating_sub(entry.size_bytes);
                freed += entry.size_bytes;
                evicted += 1;
                // #1449 / MEM-01 FIX: defer the GPU free instead of destroying
                // the acceleration structure + backing buffer immediately. The
                // previous frame's in-flight TLAS may still reference this
                // BLAS's device address — streaming runs `build_blas_batched`
                // (which calls this) in `about_to_wait` BEFORE the next
                // `draw_frame`'s fence wait, so an immediate destroy frees the
                // AS under a GPU still executing ray queries against it →
                // page fault → `VK_ERROR_DEVICE_LOST`. `tick_deferred_destroy`
                // frees the entry `DEFAULT_COUNTDOWN` (= `MAX_FRAMES_IN_FLIGHT`)
                // frames later, after the per-frame fence proves the referencing
                // frame has retired — exactly what `drop_blas` already does.
                self.pending_destroy_blas.push(entry, DEFAULT_COUNTDOWN);
            }
        }

        if evicted > 0 {
            log::info!(
                "BLAS eviction: freed {} entries ({:.1} MB), static budget: {:.1}/{:.1} MB (total {:.1} MB)",
                evicted,
                freed as f64 / (1024.0 * 1024.0),
                self.static_blas_bytes as f64 / (1024.0 * 1024.0),
                self.blas_budget_bytes as f64 / (1024.0 * 1024.0),
                self.total_blas_bytes as f64 / (1024.0 * 1024.0),
            );
            // BLAS map mutated — see #300.
            self.blas_map_generation = self.blas_map_generation.wrapping_add(1);
            // Force full TLAS rebuild next frame since BLAS addresses changed.
            for ref mut t in self.tlas.iter_mut().flatten() {
                t.needs_full_rebuild = true;
            }
        }
    }
}
