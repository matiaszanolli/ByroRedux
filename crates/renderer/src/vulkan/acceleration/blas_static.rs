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
use super::types::BlasEntry;
use super::AccelerationManager;
use crate::deferred_destroy::DEFAULT_COUNTDOWN;
use crate::mesh::GpuMesh;
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

    /// Build a BLAS for a mesh. Call after uploading the mesh to GPU.
    ///
    /// NOTE: This submits a one-time command buffer and blocks on a fence
    /// via `with_one_time_commands`. Acceptable during scene load; for
    /// streaming, batch BLAS builds into the frame's command buffer to
    /// avoid per-mesh GPU stalls. See #284 (C2-04).
    pub fn build_blas(
        &mut self,
        ctx: crate::vulkan::GpuUploadCtx,
        transfer_fence: Option<&std::sync::Mutex<vk::Fence>>,
        mesh_handle: u32,
        mesh: &GpuMesh,
        vertex_count: u32,
        index_count: u32,
    ) -> Result<()> {
        let crate::vulkan::GpuUploadCtx {
            device,
            allocator,
            queue,
            command_pool,
        } = ctx;
        // #915 / REN-D8-NEW-05 — sibling of the `build_blas_batched`
        // pre-batch eviction at line ~1354. The batched path is the
        // M40 cell-loader hot path, so eviction lives there; the
        // single-shot path here is hit by ad-hoc / UI-quad / lazy-
        // upload registrations and was missing the guard. A future
        // streaming refactor that promoted single-shot to the hot
        // path (or a 6 GB-budget GPU running near the cap) would
        // silently bypass `blas_budget_bytes` here and let the
        // static BLAS pool grow past the budget. Mirror the
        // batched-path call so eviction fires uniformly across the
        // two BLAS-creating entry points.
        // SAFETY: `device` + `allocator` are live for this call; evicted
        // entries are gated to idle >= MAX_FRAMES_IN_FLIGHT + 1, so no
        // in-flight command buffer or TLAS build references them.
        //
        // #1792 — `pending_bytes = 0`: this single-shot path hasn't sized
        // its own result buffer yet at this point, so it has nothing to
        // report as pending on top of the already-committed total.
        unsafe {
            self.evict_unused_blas(device, allocator, 0);
        }

        let vertex_stride = std::mem::size_of::<Vertex>() as vk::DeviceSize;

        // SAFETY: get_buffer_device_address requires the buffer was created with
        // SHADER_DEVICE_ADDRESS. Our vertex/index buffers are created with this flag.
        // The returned u64 address is valid for the buffer's lifetime.
        let vertex_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(
                    mesh.vertex_buffer
                        .as_ref()
                        .expect("BLAS build requires a per-mesh vertex buffer; global-only meshes are rt-disabled and must not be BLAS-built")
                        .buffer,
                ),
            )
        };
        // SAFETY: the index buffer was created with SHADER_DEVICE_ADDRESS;
        // the returned address is valid for the buffer's lifetime.
        let index_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(
                    mesh.index_buffer
                        .as_ref()
                        .expect("BLAS build requires a per-mesh index buffer; global-only meshes are rt-disabled and must not be BLAS-built")
                        .buffer,
                ),
            )
        };

        // SAFETY: DeviceOrHostAddressConstKHR is a union — we initialize the
        // device_address field because we're using device-local buffers (not host pointers).
        let triangles = vk::AccelerationStructureGeometryTrianglesDataKHR::default()
            .vertex_format(vk::Format::R32G32B32_SFLOAT)
            .vertex_data(vk::DeviceOrHostAddressConstKHR {
                device_address: vertex_address,
            })
            .vertex_stride(vertex_stride)
            .max_vertex(vertex_count.saturating_sub(1))
            .index_type(vk::IndexType::UINT32)
            .index_data(vk::DeviceOrHostAddressConstKHR {
                device_address: index_address,
            });

        let geometry = vk::AccelerationStructureGeometryKHR::default()
            .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
            .flags(vk::GeometryFlagsKHR::OPAQUE)
            .geometry(vk::AccelerationStructureGeometryDataKHR { triangles });

        let primitive_count = index_count / 3;

        let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
            .flags(STATIC_BLAS_FLAGS)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .geometries(std::slice::from_ref(&geometry));

        // Query sizes.
        let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
        // SAFETY: query-only call; `accel_loader` + `build_info`
        // (value-typed geometry, no host pointers) + `sizes` out-param
        // are live for the call; device outlives it.
        unsafe {
            self.accel_loader.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &build_info,
                &[primitive_count],
                &mut sizes,
            );
        };

        // Allocate result buffer in DEVICE_LOCAL memory (GPU-built, GPU-read only).
        let mut result_buffer = GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            sizes.acceleration_structure_size,
            vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        )?;

        // Create the acceleration structure object.
        let accel_info = vk::AccelerationStructureCreateInfoKHR::default()
            .buffer(result_buffer.buffer)
            .size(sizes.acceleration_structure_size)
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL);

        // SAFETY: `accel_info` references `result_buffer.buffer`, just
        // created with ACCELERATION_STRUCTURE_STORAGE_KHR and still live;
        // device outlives the call.
        let accel = unsafe {
            self.accel_loader
                .create_acceleration_structure(&accel_info, None)
                .context("Failed to create BLAS")?
        };

        // Wrap the remaining fallible operations in a closure so we can
        // clean up `accel` + `result_buffer` on any failure. Without this,
        // a scratch allocation or command submission error would leak both.
        let build_result = (|| -> Result<()> {
            // Reuse persisted BLAS scratch buffer; only reallocate if the current
            // one is too small for this build. Grow-only policy via shared
            // helper — see #60 / #424 SIBLING. The requested size carries
            // `scratch_alignment_padding` headroom (#1386) so the device
            // address can be rounded up to `scratch_align` below without
            // the build's scratch range overrunning the buffer.
            let scratch_size =
                sizes.build_scratch_size + scratch_alignment_padding(self.scratch_align);
            let need_new_scratch = scratch_needs_growth(
                self.blas_scratch_buffer.as_ref().map(|b| b.size),
                scratch_size,
            );

            if need_new_scratch {
                // #1782 — do NOT destroy `old` immediately. This is the
                // single-shot build path, called during cell load, but
                // `blas_scratch_buffer` is shared with the per-frame
                // skinned-BLAS refit/first-sight-build paths, whose
                // command buffer may still be in flight on the GPU
                // (recorded before this call, submitted, not yet
                // fenced) and referencing `old`'s device address as
                // build scratch. Route through the deferred-destroy
                // queue so the free waits out `MAX_FRAMES_IN_FLIGHT`
                // frames instead of racing the GPU.
                if let Some(old) = self.blas_scratch_buffer.take() {
                    self.pending_destroy_scratch.push(old, DEFAULT_COUNTDOWN);
                }
                self.blas_scratch_buffer = Some(GpuBuffer::create_device_local_uninit(
                    device,
                    allocator,
                    scratch_size,
                    vk::BufferUsageFlags::STORAGE_BUFFER
                        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                )?);
            }

            // SAFETY: scratch buffer was just created with SHADER_DEVICE_ADDRESS flag.
            // Vulkan spec requires the scratch `device_address` to be a
            // multiple of `minAccelerationStructureScratchOffsetAlignment`
            // (typically 128 or 256). `gpu-allocator` returns GpuOnly
            // allocations at >= 256 B alignment on every desktop driver we
            // ship support for, so `align_scratch_address` is a no-op there;
            // on a future misaligning driver it rounds the raw address up
            // into the headroom reserved above, enforcing
            // VUID-…-pInfos-03715 even in release (where the prior
            // `debug_assert_scratch_aligned` compiled out). See #1386 / #659.
            let raw_scratch = unsafe {
                device.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default()
                        .buffer(self.blas_scratch_buffer.as_ref().unwrap().buffer),
                )
            };
            let scratch_address = align_scratch_address(raw_scratch, self.scratch_align);

            // Build the BLAS via one-time command buffer. Flags must
            // match the size-query above per Vulkan spec.
            // SAFETY: DeviceOrHostAddressKHR union — device_address field used for device builds.
            let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                .flags(STATIC_BLAS_FLAGS)
                .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                .dst_acceleration_structure(accel)
                .geometries(std::slice::from_ref(&geometry))
                .scratch_data(vk::DeviceOrHostAddressKHR {
                    device_address: scratch_address,
                });

            let range_info = vk::AccelerationStructureBuildRangeInfoKHR::default()
                .primitive_count(primitive_count)
                .primitive_offset(0)
                .first_vertex(0);

            submit_one_time(device, queue, command_pool, transfer_fence, |cmd| {
                unsafe {
                    self.accel_loader.cmd_build_acceleration_structures(
                        cmd,
                        &[build_info],
                        &[std::slice::from_ref(&range_info)],
                    );
                }
                Ok(())
            })
        })();

        if let Err(e) = build_result {
            // Clean up the accel structure and result buffer that were
            // already created before the build failed.
            // SAFETY: `accel` was created above and the build that would
            // reference it failed before any command buffer recorded it,
            // so no in-flight build aliases it; device is live.
            unsafe {
                self.accel_loader
                    .destroy_acceleration_structure(accel, None);
            }
            result_buffer.destroy(device, allocator);
            return Err(e);
        }

        // Get the BLAS device address.
        // SAFETY: `accel` is the live BLAS just built; query-only call;
        // device outlives it.
        let device_address = unsafe {
            self.accel_loader.get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                    .acceleration_structure(accel),
            )
        };

        // Store BLAS entry (scratch buffer is retained for next build).
        let handle = mesh_handle as usize;
        let blas_size = result_buffer.size;
        while self.blas_entries.len() <= handle {
            self.blas_entries.push(None);
        }
        self.total_blas_bytes += blas_size;
        self.static_blas_bytes += blas_size;
        self.blas_entries[handle] = Some(BlasEntry {
            accel,
            buffer: result_buffer,
            device_address,
            last_used_frame: self.frame_counter,
            size_bytes: blas_size,
            build_scratch_size: sizes.build_scratch_size,
            // Static BLAS never refit; the field stays at zero for
            // their entire lifetime. See #679.
            refit_count: 0,
            built_vertex_count: vertex_count,
            built_index_count: index_count,
            // #1145 — record for symmetry / telemetry. Static BLAS
            // never refit so this field is read-only here.
            built_flags: STATIC_BLAS_FLAGS,
        });
        // BLAS map mutated — see #300.
        self.blas_map_generation = self.blas_map_generation.wrapping_add(1);

        Ok(())
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
        meshes: &[(u32, &GpuMesh, u32, u32)], // (mesh_handle, mesh, vertex_count, index_count)
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

        for &(_mesh_handle, mesh, vertex_count, _index_count) in meshes {
            // SAFETY: the per-mesh vertex buffer was created with
            // SHADER_DEVICE_ADDRESS; the returned address is valid for the
            // buffer's lifetime.
            let vertex_address = unsafe {
                device.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default().buffer(
                    mesh.vertex_buffer
                        .as_ref()
                        .expect("BLAS build requires a per-mesh vertex buffer; global-only meshes are rt-disabled and must not be BLAS-built")
                        .buffer,
                ),
                )
            };
            // SAFETY: the per-mesh index buffer was created with
            // SHADER_DEVICE_ADDRESS; the returned address is valid for the
            // buffer's lifetime.
            let index_address = unsafe {
                device.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default().buffer(
                    mesh.index_buffer
                        .as_ref()
                        .expect("BLAS build requires a per-mesh index buffer; global-only meshes are rt-disabled and must not be BLAS-built")
                        .buffer,
                ),
                )
            };

            let triangles = vk::AccelerationStructureGeometryTrianglesDataKHR::default()
                .vertex_format(vk::Format::R32G32B32_SFLOAT)
                .vertex_data(vk::DeviceOrHostAddressConstKHR {
                    device_address: vertex_address,
                })
                .vertex_stride(vertex_stride)
                .max_vertex(vertex_count.saturating_sub(1))
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
        // SAFETY: `device` + `allocator` are live; the prepared buffers
        // for this batch are not yet in `blas_entries`, and eviction only
        // frees entries past the idle threshold, so no in-flight build is
        // aliased.
        //
        // #1792 — `pending_bytes = 0`: nothing in this batch has been
        // sized yet at this point (the loop below hasn't run).
        unsafe {
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
        let mut pending_bytes: vk::DeviceSize = 0;
        // Now build geometries referencing the stored triangles data.
        for (idx, &(mesh_handle, _mesh, vertex_count, index_count)) in meshes.iter().enumerate() {
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
                unsafe {
                    self.evict_unused_blas(device, allocator, pending_bytes);
                }
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
        let alloc_compact = || -> Result<(Vec<CompactedBlas>, u64, u64)> {
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

            // Tuple: (mesh_handle, compacted accel, compacted buffer,
            // build_scratch_size, vertex_count, index_count). Scratch
            // size is propagated from `prepared` so the final
            // `BlasEntry` can remember what scratch this mesh consumed
            // at build time (#495); vertex/index counts are propagated
            // for the refit-counts VUID check (#907 — static BLAS
            // never refit but we pin the counts for symmetry).
            let mut compact_accels: Vec<(
                u32,
                vk::AccelerationStructureKHR,
                GpuBuffer,
                vk::DeviceSize,
                u32,
                u32,
            )> = Vec::with_capacity(prepared.len());

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
                            // cleanup loop won't see it — destroy it locally
                            // before bubbling so the OOM path is leak-free.
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

            Ok((compact_accels, total_before, total_after))
        };

        let (compact_accels, total_before, total_after) = match alloc_compact() {
            Ok(v) => v,
            Err(e) => {
                // Roll back: destroy the originals (phase 7's job on the
                // happy path) and the query pool. Partial phase-6 compact
                // state was already cleaned up inside `alloc_compact`.
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
    /// # Safety
    ///
    /// Caller must ensure `device` and `allocator` are valid and live. (The
    /// "no in-flight references" precondition is no longer required — the
    /// deferred-destroy queue guarantees it. The `unsafe` marker is retained
    /// only for call-site signature stability; this body performs no unsafe
    /// operation now that the destroy is deferred.)
    pub unsafe fn evict_unused_blas(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        pending_bytes: vk::DeviceSize,
    ) {
        // The GPU free is now deferred (see the loop body), so this function no
        // longer touches `device`/`allocator` directly — `tick_deferred_destroy`
        // does. The params are retained so the call sites
        // (`build_blas`/`build_blas_batched`/`draw.rs`) keep a stable signature;
        // the `unsafe` marker is likewise vestigial and can be dropped in a
        // follow-up once a non-`unsafe` signature is threaded through callers.
        let _ = (device, allocator);

        if !blas_over_budget(self.static_blas_bytes, pending_bytes, self.blas_budget_bytes) {
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
            if !blas_over_budget(self.static_blas_bytes, pending_bytes, self.blas_budget_bytes) {
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
