//! Static (mesh-keyed) BLAS lifecycle and builds.
//!
//! Covers the BLAS path that lives in [`super::AccelerationManager::blas_entries`]:
//! single-mesh + batched builds, deferred destroy, eviction. Skinned
//! (per-entity) BLAS live in [`super::blas_skinned`].

use super::super::descriptors::memory_barrier;
use super::super::allocator::SharedAllocator;
use super::super::buffer::GpuBuffer;
use super::super::sync::MAX_FRAMES_IN_FLIGHT;
use super::constants::BATCH_EVICTION_CHECK_INTERVAL;
use super::predicates::{scratch_needs_growth, should_evict_mid_batch, submit_one_time};
use super::types::BlasEntry;
use super::AccelerationManager;
use crate::deferred_destroy::DEFAULT_COUNTDOWN;
use crate::mesh::GpuMesh;
use crate::vertex::Vertex;
use anyhow::{Context, Result};
use ash::vk;

impl AccelerationManager {
    /// Queue a BLAS for deferred destruction.
    ///
    /// Unlike [`evict_unused_blas`](Self::evict_unused_blas) — which runs
    /// only on entries idle for `MAX_FRAMES_IN_FLIGHT` frames and so can
    /// destroy immediately — `drop_blas` is called by the cell loader on
    /// unload and the entry may still be in-flight. The entry moves to
    /// `pending_destroy_blas` and the actual `VkAccelerationStructureKHR`
    /// + buffer destruction is delayed until the countdown expires in
    /// [`tick_deferred_destroy`](Self::tick_deferred_destroy).
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
        for tlas_slot in &mut self.tlas {
            if let Some(ref mut t) = tlas_slot {
                t.needs_full_rebuild = true;
            }
        }
    }

    /// Drain and destroy BLAS entries whose defer countdown has reached
    /// zero. Call once per frame alongside
    /// `MeshRegistry::tick_deferred_destroy`.
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
            unsafe {
                accel_loader.destroy_acceleration_structure(entry.accel, None);
            }
            entry.buffer.destroy(device, allocator);
        });
    }

    /// Number of entries currently waiting in `pending_destroy_blas`.
    /// Surfaced for [`drain_pending_destroys`]'s unit test and shutdown
    /// telemetry — the count must reach zero after a drain. See #732.
    pub fn pending_destroy_blas_count(&self) -> usize {
        self.pending_destroy_blas.len()
    }

    /// Build a BLAS for a mesh. Call after uploading the mesh to GPU.
    ///
    /// NOTE: This submits a one-time command buffer and blocks on a fence
    /// via `with_one_time_commands`. Acceptable during scene load; for
    /// streaming, batch BLAS builds into the frame's command buffer to
    /// avoid per-mesh GPU stalls. See #284 (C2-04).
    pub fn build_blas(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        transfer_fence: Option<&std::sync::Mutex<vk::Fence>>,
        mesh_handle: u32,
        mesh: &GpuMesh,
        vertex_count: u32,
        index_count: u32,
    ) -> Result<()> {
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
        unsafe {
            self.evict_unused_blas(device, allocator);
        }

        let vertex_stride = std::mem::size_of::<Vertex>() as vk::DeviceSize;

        // SAFETY: get_buffer_device_address requires the buffer was created with
        // SHADER_DEVICE_ADDRESS. Our vertex/index buffers are created with this flag.
        // The returned u64 address is valid for the buffer's lifetime.
        let vertex_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(mesh.vertex_buffer.buffer),
            )
        };
        let index_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(mesh.index_buffer.buffer),
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

        // Match the flag set used by the batched-build path (#658) so
        // the single-shot path stays in lockstep. ALLOW_COMPACTION is
        // a no-op on its own — no compact-copy phase is wired in here
        // — but having the flag set means a future caller that wants
        // to compact this BLAS can issue
        // `cmd_copy_acceleration_structure(MODE = COMPACT)` against it
        // without rebuilding from scratch. Today this path is reached
        // only by UI-quad / single-mesh registration where compaction
        // would save trivial bytes; routing an RT mesh through here
        // (e.g. lazy first-sight upload) without the flag would
        // silently consume the BLAS budget twice as fast as a
        // batched-path peer.
        //
        // REN-D8-NEW-06 (audit 2026-05-09) flagged the flag as
        // "wasted" because no caller currently runs the compact
        // pass. The lockstep with the batched path is the load-
        // bearing reason for keeping it — drop here without
        // dropping at the batched build site (line 1534) would
        // create an asymmetric compaction policy that's harder to
        // reason about than one flag value across both paths.
        // When the compact pass lands, it lights up on both paths
        // simultaneously.
        const STATIC_BLAS_FLAGS: vk::BuildAccelerationStructureFlagsKHR =
            vk::BuildAccelerationStructureFlagsKHR::from_raw(
                vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE.as_raw()
                    | vk::BuildAccelerationStructureFlagsKHR::ALLOW_COMPACTION.as_raw(),
            );
        let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
            .flags(STATIC_BLAS_FLAGS)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .geometries(std::slice::from_ref(&geometry));

        // Query sizes.
        let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
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
            // helper — see #60 / #424 SIBLING.
            let need_new_scratch = scratch_needs_growth(
                self.blas_scratch_buffer.as_ref().map(|b| b.size),
                sizes.build_scratch_size,
            );

            if need_new_scratch {
                if let Some(mut old) = self.blas_scratch_buffer.take() {
                    old.destroy(device, allocator);
                }
                self.blas_scratch_buffer = Some(GpuBuffer::create_device_local_uninit(
                    device,
                    allocator,
                    sizes.build_scratch_size,
                    vk::BufferUsageFlags::STORAGE_BUFFER
                        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                )?);
            }

            // SAFETY: scratch buffer was just created with SHADER_DEVICE_ADDRESS flag.
            // The `scratch_address` is required by Vulkan spec to be a
            // multiple of `minAccelerationStructureScratchOffsetAlignment`
            // (typically 128 or 256). `gpu-allocator` returns GpuOnly
            // allocations at >= 256 B alignment on every desktop driver
            // we ship support for, but nothing in the allocator API
            // guarantees it — the `debug_assert_scratch_aligned` call
            // catches a future driver / mobile GPU regression at the
            // earliest possible point. See #659 / #260 R-05.
            let scratch_address = unsafe {
                device.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default()
                        .buffer(self.blas_scratch_buffer.as_ref().unwrap().buffer),
                )
            };
            self.debug_assert_scratch_aligned(scratch_address, "build_blas");

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
            unsafe {
                self.accel_loader
                    .destroy_acceleration_structure(accel, None);
            }
            result_buffer.destroy(device, allocator);
            return Err(e);
        }

        // Get the BLAS device address.
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
            let vertex_address = unsafe {
                device.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default().buffer(mesh.vertex_buffer.buffer),
                )
            };
            let index_address = unsafe {
                device.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default().buffer(mesh.index_buffer.buffer),
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
        unsafe {
            self.evict_unused_blas(device, allocator);
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
            if idx > 0 && idx % BATCH_EVICTION_CHECK_INTERVAL == 0 {
                if should_evict_mid_batch(
                    self.static_blas_bytes,
                    pending_bytes,
                    self.blas_budget_bytes,
                ) {
                    // SAFETY: prepared buffers for this batch are local
                    // to `prepared` and not yet in `self.blas_entries`,
                    // so `evict_unused_blas` cannot touch them — it only
                    // frees entries in `blas_entries` that are past the
                    // idle threshold.
                    unsafe {
                        self.evict_unused_blas(device, allocator);
                    }
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
                .flags(
                    vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE
                        | vk::BuildAccelerationStructureFlagsKHR::ALLOW_COMPACTION,
                )
                .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                .geometries(std::slice::from_ref(&geometry));

            let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
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

            let accel = unsafe {
                match self
                    .accel_loader
                    .create_acceleration_structure(&accel_info, None)
                {
                    Ok(a) => a,
                    Err(e) => {
                        result_buffer.destroy(device, allocator);
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
        // policy via shared helper — see #60 / #424 SIBLING.
        let need_new_scratch = scratch_needs_growth(
            self.blas_scratch_buffer.as_ref().map(|b| b.size),
            max_scratch_size,
        );

        if need_new_scratch {
            if let Some(mut old) = self.blas_scratch_buffer.take() {
                old.destroy(device, allocator);
            }
            self.blas_scratch_buffer = Some(GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                max_scratch_size,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            )?);
        }

        let scratch_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default()
                    .buffer(self.blas_scratch_buffer.as_ref().unwrap().buffer),
            )
        };
        self.debug_assert_scratch_aligned(scratch_address, "build_blas_batched");

        // Phase 3: Create query pool for compacted size readback.
        let n = prepared.len() as u32;
        let query_pool_info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::ACCELERATION_STRUCTURE_COMPACTED_SIZE_KHR)
            .query_count(n);
        let query_pool = unsafe {
            device
                .create_query_pool(&query_pool_info, None)
                .context("Failed to create compaction query pool")?
        };
        // Reset the query pool before use (required by Vulkan spec).
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
                    .flags(
                        vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE
                            | vk::BuildAccelerationStructureFlagsKHR::ALLOW_COMPACTION,
                    )
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
            unsafe {
                memory_barrier(
                    device, cmd,
                    vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                    vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
                    vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                    vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR,
                );
            }

            // Query compacted sizes for all built BLAS.
            let accel_handles: Vec<vk::AccelerationStructureKHR> =
                prepared.iter().map(|p| p.accel).collect();
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
                unsafe {
                    self.accel_loader
                        .destroy_acceleration_structure(p.accel, None);
                }
                p.buffer.destroy(device, allocator);
            }
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
        let alloc_compact = || -> Result<(
            Vec<(
                u32,
                vk::AccelerationStructureKHR,
                GpuBuffer,
                vk::DeviceSize,
                u32,
                u32,
            )>,
            u64,
            u64,
        )> {
            let mut compacted_sizes = vec![0u64; prepared.len()];
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
                    unsafe {
                        self.accel_loader
                            .destroy_acceleration_structure(p.accel, None);
                    }
                    p.buffer.destroy(device, allocator);
                }
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

                unsafe {
                    self.accel_loader
                        .cmd_copy_acceleration_structure(cmd, &copy_info);
                }
            }
            Ok(())
        });

        // Destroy the query pool — no longer needed.
        unsafe {
            device.destroy_query_pool(query_pool, None);
        }

        if let Err(e) = copy_result {
            // Clean up both original and compact structures on failure.
            for mut p in prepared {
                unsafe {
                    self.accel_loader
                        .destroy_acceleration_structure(p.accel, None);
                }
                p.buffer.destroy(device, allocator);
            }
            for (_, accel, mut buf, _, _, _) in compact_accels {
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

    /// Evict unused BLAS entries when static BLAS memory exceeds the budget.
    ///
    /// Entries unused for more than `min_idle_frames` frames are candidates.
    /// Eviction is LRU — the least recently used entries are destroyed first.
    /// Only entries unused for >= MAX_FRAMES_IN_FLIGHT frames are safe to
    /// evict (guarantees no in-flight TLAS references them).
    ///
    /// The budget compare uses `static_blas_bytes`, NOT `total_blas_bytes`,
    /// because skinned per-entity BLAS aren't eviction candidates (see
    /// `static_blas_bytes` doc on the struct field for details / #920).
    ///
    /// Unlike [`drop_blas`](Self::drop_blas), eviction destroys the
    /// `VkAccelerationStructureKHR` immediately rather than routing it
    /// through `pending_destroy_blas`. That is sound because `min_idle`
    /// (declared inline below) is strictly greater than
    /// `MAX_FRAMES_IN_FLIGHT`, so any candidate's `last_used_frame`
    /// predates the oldest in-flight frame's fence — its command buffer
    /// has retired and the GPU no longer references the AS. The
    /// `const_assert` inside the function pins that invariant: if
    /// anyone raises `MAX_FRAMES_IN_FLIGHT` without bumping `min_idle`,
    /// the workspace fails to compile instead of silently introducing a
    /// use-after-free window (REN-D8-NEW-16 / #960).
    pub unsafe fn evict_unused_blas(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        if self.static_blas_bytes <= self.blas_budget_bytes {
            return;
        }

        // REN-D8-NEW-16 / #960 — `evict_unused_blas` destroys the
        // `VkAccelerationStructureKHR` immediately (no
        // `pending_destroy_blas` round-trip), so the gate constant has
        // to outrun the deepest in-flight frame slot. Today the gate is
        // `MAX_FRAMES_IN_FLIGHT + 1 = 3` and `MAX_FRAMES_IN_FLIGHT = 2`
        // (pinned by `sync.rs:32`), but a future bump to either side
        // could silently close the safety window — e.g. someone
        // hardcodes `MIN_IDLE_FRAMES = 3` and a later contributor
        // raises `MAX_FRAMES_IN_FLIGHT` to 3. The const_assert below
        // ties the two values together at compile time so that
        // mismatch fails the workspace build instead of producing a
        // use-after-free.
        const MIN_IDLE_FRAMES: u64 = MAX_FRAMES_IN_FLIGHT as u64 + 1;
        const _: () = assert!(
            MIN_IDLE_FRAMES > MAX_FRAMES_IN_FLIGHT as u64,
            "evict_unused_blas immediate-destroy requires \
             MIN_IDLE_FRAMES > MAX_FRAMES_IN_FLIGHT; either widen the \
             gate or route eviction through pending_destroy_blas",
        );
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
            if self.static_blas_bytes <= self.blas_budget_bytes {
                break;
            }
            if let Some(mut entry) = self.blas_entries[idx].take() {
                self.accel_loader
                    .destroy_acceleration_structure(entry.accel, None);
                entry.buffer.destroy(device, allocator);
                self.total_blas_bytes = self.total_blas_bytes.saturating_sub(entry.size_bytes);
                self.static_blas_bytes = self.static_blas_bytes.saturating_sub(entry.size_bytes);
                freed += entry.size_bytes;
                evicted += 1;
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
            for slot in &mut self.tlas {
                if let Some(ref mut t) = slot {
                    t.needs_full_rebuild = true;
                }
            }
        }
    }
}
