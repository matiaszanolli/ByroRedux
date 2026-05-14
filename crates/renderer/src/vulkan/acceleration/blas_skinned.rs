//! Skinned (per-entity) BLAS lifecycle and builds.
//!
//! Covers the path that lives in [`super::AccelerationManager::skinned_blas`]:
//! one BLAS per animated instance, refit each frame against the
//! SkinComputePipeline output buffer. Static (mesh-keyed) BLAS live in
//! [`super::blas_static`]. See M29 Phase 2.

use super::super::allocator::SharedAllocator;
use super::super::buffer::GpuBuffer;
use super::super::sync::MAX_FRAMES_IN_FLIGHT;
use super::constants::UPDATABLE_AS_FLAGS;
use super::predicates::{
    is_scratch_aligned, scratch_needs_growth, should_rebuild_skinned_blas_after, submit_one_time,
    validate_refit_counts,
};
use super::types::BlasEntry;
use super::AccelerationManager;
use crate::vertex::Vertex;
use anyhow::{Context, Result};
use ash::vk;
use byroredux_core::ecs::storage::EntityId;

impl AccelerationManager {
    /// M29 Phase 2: build a per-skinned-entity BLAS from the entity's
    /// SkinComputePipeline output buffer + the mesh's existing index
    /// buffer. Sets `ALLOW_UPDATE` on the build flags so subsequent
    /// per-frame `cmd_build_acceleration_structures(mode = UPDATE)`
    /// is legal (refit-in-place against the same vertex source —
    /// topology never changes for a skinned mesh).
    ///
    /// `vertex_buffer` is the SkinSlot's output_buffer and must have
    /// been created with `SHADER_DEVICE_ADDRESS +
    /// ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR + STORAGE_BUFFER`
    /// (skin_compute.rs already does this). `index_buffer` is reused
    /// from the bind-pose `GpuMesh.index_buffer` — topology stays
    /// identical across frames so we don't need a per-entity index
    /// buffer. Caller is responsible for inserting a
    /// COMPUTE_SHADER_WRITE → ACCELERATION_STRUCTURE_BUILD_INPUT_READ
    /// barrier on `vertex_buffer` before this build runs.
    ///
    /// Initial build copies bind-pose vertices through the SkinSlot's
    /// output (the caller dispatches the compute pass first), so the
    /// BLAS is correct from frame 0. Subsequent frames must call
    /// [`Self::refit_skinned_blas`] in `mode = UPDATE`.
    pub fn build_skinned_blas(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        transfer_fence: Option<&std::sync::Mutex<vk::Fence>>,
        entity_id: EntityId,
        vertex_buffer: vk::Buffer,
        vertex_count: u32,
        index_buffer: vk::Buffer,
        index_count: u32,
    ) -> Result<()> {
        let vertex_stride = std::mem::size_of::<Vertex>() as vk::DeviceSize;
        let vertex_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(vertex_buffer),
            )
        };
        let index_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(index_buffer),
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
        let geometry = vk::AccelerationStructureGeometryKHR::default()
            .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
            // Skinned meshes are typically opaque; if any actor mesh
            // ever ships alpha-tested triangles, the per-instance
            // `two_sided` flag in the TLAS instance still toggles
            // backface-cull for that draw. Leaving OPAQUE here matches
            // the static `build_blas` path.
            .flags(vk::GeometryFlagsKHR::OPAQUE)
            .geometry(vk::AccelerationStructureGeometryDataKHR { triangles });
        let primitive_count = index_count / 3;

        // Build flags: see `UPDATABLE_AS_FLAGS` for the shared
        // PREFER_FAST_TRACE | ALLOW_UPDATE rationale (#679 / REN-D8-NEW-08:
        // skinned BLAS refits in-place ~600 frames between full builds, so
        // trace cost dominates by ~6 orders of magnitude). #958 lifted the
        // four UPDATE-target call sites to the shared constant to enforce
        // VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03667 by
        // construction.
        let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
            .flags(UPDATABLE_AS_FLAGS)
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
            self.accel_loader
                .create_acceleration_structure(&accel_info, None)
                .context("create skinned BLAS")?
        };

        let build_result = (|| -> Result<()> {
            // Skinned BLAS uses the SAME shared scratch buffer as
            // static builds — the build still runs in a one-time
            // command buffer with a fence wait so there's no overlap.
            // `update_scratch_size` for refits is at most
            // `build_scratch_size` per Vulkan spec, so the existing
            // grow-only policy stays correct.
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
            let scratch_address = unsafe {
                device.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default()
                        .buffer(self.blas_scratch_buffer.as_ref().unwrap().buffer),
                )
            };
            self.debug_assert_scratch_aligned(scratch_address, "build_skinned_blas");
            let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                .flags(UPDATABLE_AS_FLAGS)
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
            unsafe {
                self.accel_loader
                    .destroy_acceleration_structure(accel, None);
            }
            result_buffer.destroy(device, allocator);
            return Err(e);
        }

        let device_address = unsafe {
            self.accel_loader.get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                    .acceleration_structure(accel),
            )
        };

        let blas_size = result_buffer.size;
        // Skinned BLAS are NOT eviction candidates (lifecycle is tied to
        // entity visibility — see `drop_skinned_blas`), so do NOT update
        // `static_blas_bytes`. See #920 for the LRU thrash this prevents.
        self.total_blas_bytes += blas_size;
        self.skinned_blas.insert(
            entity_id,
            BlasEntry {
                accel,
                buffer: result_buffer,
                device_address,
                last_used_frame: self.frame_counter,
                size_bytes: blas_size,
                build_scratch_size: sizes.build_scratch_size,
                // Fresh BUILD resets the refit chain — the new BVH
                // bounds tightly fit the current pose. See #679.
                refit_count: 0,
                // #907 — pin the counts used at BUILD time so the
                // next `refit_skinned_blas` can validate that its
                // caller-supplied counts match (Vulkan VUID 03667).
                built_vertex_count: vertex_count,
                built_index_count: index_count,
            },
        );
        self.blas_map_generation = self.blas_map_generation.wrapping_add(1);

        Ok(())
    }

    /// Batched first-sight skinned BLAS builds recorded onto a caller-
    /// supplied command buffer — eliminates the per-NPC host fence
    /// stall that [`Self::build_skinned_blas`] pays through
    /// `submit_one_time`. See #911 / REN-D5-NEW-02.
    ///
    /// Three phases:
    ///   1. Per-entity sizing + result-buffer + accel-structure
    ///      allocation. A per-entity failure (typically OOM) is
    ///      reported in the result vec for that entity and the batch
    ///      continues with the remaining entities.
    ///   2. Resize the shared `blas_scratch_buffer` ONCE to fit the
    ///      max scratch demand across the batch — this is the key
    ///      safety property. The naive "record N builds back-to-back
    ///      on `cmd`, each inline-resizing scratch" path is UB: an
    ///      earlier `cmd_build_acceleration_structures` call's
    ///      `scratch_data` device address points at memory the next
    ///      build's resize then freed. With sizing done upfront the
    ///      scratch address is stable across all recorded builds.
    ///   3. Per-entity recording into `cmd`, separated by
    ///      `record_scratch_serialize_barrier` calls because all
    ///      builds share scratch (`AS_WRITE → AS_WRITE` at
    ///      `ACCELERATION_STRUCTURE_BUILD_KHR`).
    ///
    /// The caller must have emitted a
    /// `COMPUTE_SHADER_WRITE → AS_BUILD_INPUT_READ` barrier on the
    /// vertex buffers before this call, exactly as for
    /// [`Self::refit_skinned_blas`]. The intended call site (per-
    /// frame `cmd` in `draw_frame`) gets this via the same compute→AS
    /// barrier that already precedes the refit loop.
    ///
    /// Returns `(entity_id, Result<()>)` pairs in batch order so the
    /// caller can correlate coverage counters per entity.
    pub fn build_skinned_blas_batched_on_cmd(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        cmd: vk::CommandBuffer,
        entities: &[(EntityId, vk::Buffer, u32, vk::Buffer, u32)],
    ) -> Vec<(EntityId, Result<()>)> {
        if entities.is_empty() {
            return Vec::new();
        }

        struct PreparedSkinned {
            entity_id: EntityId,
            accel: vk::AccelerationStructureKHR,
            buffer: GpuBuffer,
            geometry: vk::AccelerationStructureGeometryKHR<'static>,
            primitive_count: u32,
            build_scratch_size: vk::DeviceSize,
            vertex_count: u32,
            index_count: u32,
        }

        let vertex_stride = std::mem::size_of::<Vertex>() as vk::DeviceSize;
        // Flags: shared `UPDATABLE_AS_FLAGS` — see #958 / REN-D8-NEW-14.

        let mut prepared: Vec<PreparedSkinned> = Vec::with_capacity(entities.len());
        let mut results: Vec<(EntityId, Result<()>)> = Vec::with_capacity(entities.len());
        let mut max_scratch_size: vk::DeviceSize = 0;

        for &(entity_id, vertex_buffer, vertex_count, index_buffer, index_count) in entities {
            let vertex_address = unsafe {
                device.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default().buffer(vertex_buffer),
                )
            };
            let index_address = unsafe {
                device.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default().buffer(index_buffer),
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
            // SAFETY: the geometry union holds only value-typed fields
            // (u64 device addresses, format/index-type enums) — no
            // host pointers or Rust borrows. The `'static` lifetime
            // annotation on `PreparedSkinned::geometry` mirrors the
            // same invariant established in `build_blas_batched`
            // (SAFE-21).
            let geometry = vk::AccelerationStructureGeometryKHR::default()
                .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
                .flags(vk::GeometryFlagsKHR::OPAQUE)
                .geometry(vk::AccelerationStructureGeometryDataKHR { triangles });
            let primitive_count = index_count / 3;

            let size_build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                .flags(UPDATABLE_AS_FLAGS)
                .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                .geometries(std::slice::from_ref(&geometry));

            let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
            unsafe {
                self.accel_loader.get_acceleration_structure_build_sizes(
                    vk::AccelerationStructureBuildTypeKHR::DEVICE,
                    &size_build_info,
                    &[primitive_count],
                    &mut sizes,
                );
            }

            let mut result_buffer = match GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                sizes.acceleration_structure_size,
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            ) {
                Ok(b) => b,
                Err(e) => {
                    results.push((entity_id, Err(e)));
                    continue;
                }
            };
            let accel_info = vk::AccelerationStructureCreateInfoKHR::default()
                .buffer(result_buffer.buffer)
                .size(sizes.acceleration_structure_size)
                .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL);
            let accel = match unsafe {
                self.accel_loader
                    .create_acceleration_structure(&accel_info, None)
            } {
                Ok(a) => a,
                Err(e) => {
                    result_buffer.destroy(device, allocator);
                    results.push((entity_id, Err(anyhow::anyhow!("create skinned BLAS: {e}"))));
                    continue;
                }
            };

            max_scratch_size = max_scratch_size.max(sizes.build_scratch_size);
            prepared.push(PreparedSkinned {
                entity_id,
                accel,
                buffer: result_buffer,
                geometry,
                primitive_count,
                build_scratch_size: sizes.build_scratch_size,
                vertex_count,
                index_count,
            });
        }

        if prepared.is_empty() {
            return results;
        }

        // Phase 2: ensure shared scratch buffer is sized for the
        // largest build in the batch. Grow-only via shared helper —
        // see #60 / #424 SIBLING. Critical: this runs BEFORE any
        // `cmd_build_acceleration_structures` is recorded so the
        // `scratch_address` captured below stays valid for every
        // recorded build at submit time.
        let need_new_scratch = scratch_needs_growth(
            self.blas_scratch_buffer.as_ref().map(|b| b.size),
            max_scratch_size,
        );
        if need_new_scratch {
            if let Some(mut old) = self.blas_scratch_buffer.take() {
                old.destroy(device, allocator);
            }
            match GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                max_scratch_size,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            ) {
                Ok(b) => {
                    self.blas_scratch_buffer = Some(b);
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    for mut p in prepared {
                        unsafe {
                            self.accel_loader
                                .destroy_acceleration_structure(p.accel, None);
                        }
                        p.buffer.destroy(device, allocator);
                        results.push((
                            p.entity_id,
                            Err(anyhow::anyhow!("scratch grow failed: {err_msg}")),
                        ));
                    }
                    return results;
                }
            }
        }
        let scratch_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default()
                    .buffer(self.blas_scratch_buffer.as_ref().unwrap().buffer),
            )
        };
        self.debug_assert_scratch_aligned(scratch_address, "build_skinned_blas_batched_on_cmd");

        // Phase 3: record builds with inter-build scratch-serialise
        // barriers. The caller-supplied COMPUTE→AS_BUILD barrier on
        // the vertex buffers is a precondition; this method does not
        // re-emit it. Subsequent refit/TLAS-build call sites either
        // emit their own scratch-serialise barrier internally
        // (`refit_skinned_blas`) or read AS data after a downstream
        // AS_WRITE→AS_READ barrier (`build_tlas`'s consumer).
        for (i, p) in prepared.iter().enumerate() {
            if i > 0 {
                self.record_scratch_serialize_barrier(device, cmd);
            }
            let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                .flags(UPDATABLE_AS_FLAGS)
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

        // Phase 4: register prepared entries into `skinned_blas`. The
        // BLAS is referenced by command-buffer state recorded above,
        // so it must outlive the submission — which it will, since
        // we keep ownership in `skinned_blas` until `drop_skinned_blas`
        // routes it through `pending_destroy_blas` with the
        // `MAX_FRAMES_IN_FLIGHT` countdown.
        for p in prepared {
            let device_address = unsafe {
                self.accel_loader.get_acceleration_structure_device_address(
                    &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                        .acceleration_structure(p.accel),
                )
            };
            let blas_size = p.buffer.size;
            // Skinned BLAS are NOT eviction candidates — lifecycle is
            // tied to entity visibility, not the static budget. Total
            // counter bumps; `static_blas_bytes` stays untouched. See
            // #920 / counterpart in `build_skinned_blas`.
            self.total_blas_bytes += blas_size;
            self.skinned_blas.insert(
                p.entity_id,
                BlasEntry {
                    accel: p.accel,
                    buffer: p.buffer,
                    device_address,
                    last_used_frame: self.frame_counter,
                    size_bytes: blas_size,
                    build_scratch_size: p.build_scratch_size,
                    refit_count: 0,
                    built_vertex_count: p.vertex_count,
                    built_index_count: p.index_count,
                },
            );
            results.push((p.entity_id, Ok(())));
        }
        self.blas_map_generation = self.blas_map_generation.wrapping_add(1);
        results
    }

    /// Refit an existing skinned BLAS in-place against an updated
    /// vertex buffer. Topology is unchanged; only the vertex positions
    /// shift. `cmd` must already have a
    /// COMPUTE_SHADER_WRITE → ACCELERATION_STRUCTURE_BUILD_INPUT_READ
    /// barrier on `vertex_buffer` recorded before this call.
    ///
    /// Refit cost is much lower than full rebuild but produces a BLAS
    /// with somewhat reduced ray-trace efficiency over time. Bethesda
    /// per-frame skin deltas are small so quality holds; if a session
    /// reveals visible degradation, the caller can periodically
    /// destroy the entry + call [`Self::build_skinned_blas`] again
    /// to start fresh (deferred to a follow-up).
    ///
    /// # Safety
    /// `cmd` must be a recording command buffer; `vertex_buffer` must
    /// already have the COMPUTE_SHADER_WRITE → AS_BUILD_INPUT_READ
    /// barrier in place. The shared `blas_scratch_buffer` serialise
    /// barrier (`AS_WRITE → AS_WRITE`) is now emitted as the first
    /// statement of this function — REN-D8-NEW-15 (audit
    /// 2026-05-11). Pre-fix this was a caller-side precondition
    /// documented but unenforced; the next refactor adding a 2nd
    /// refit call site could have silently dropped it. The barrier
    /// is idempotent (`MEMORY_READ | MEMORY_WRITE → MEMORY_READ |
    /// MEMORY_WRITE`), so any redundant caller-side emission stays
    /// harmless. See #644 / MEM-2-2 for the original landing.
    pub unsafe fn refit_skinned_blas(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        entity_id: EntityId,
        vertex_buffer: vk::Buffer,
        vertex_count: u32,
        index_buffer: vk::Buffer,
        index_count: u32,
    ) -> Result<()> {
        // #983 / REN-D8-NEW-15 — Self-emitted scratch-serialize
        // barrier. The shared `blas_scratch_buffer` may have been
        // written by a sync `build_skinned_blas` / `build_blas_batched`
        // earlier this frame in a different submission, and the host
        // fence-wait between submissions does NOT establish a
        // device-side memory dependency for this submission. Moving
        // the barrier inside the callee makes the precondition
        // load-bearing in code rather than docstring; the existing
        // caller-side emission at `context/draw.rs` becomes a
        // harmless idempotent duplicate. See #644 / MEM-2-2.
        self.record_scratch_serialize_barrier(device, cmd);

        // Capture scratch_align before the mutable borrow on
        // `self.skinned_blas` so the alignment assert further down
        // doesn't try to re-borrow `&self` (#659).
        let scratch_align = self.scratch_align;
        // #907 — validate that the caller-supplied counts match the
        // ones the original fresh BUILD was sized for, BEFORE the
        // mutable borrow on `self.skinned_blas`. A mismatch would
        // trip
        // `VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03667`
        // and silently corrupt the BVH on NVIDIA (driver behaviour
        // for that VUID is undefined). Today no in-engine path
        // remaps `entity_id → mesh` between frames; this defends
        // against future mod swap / LOD switch / mesh hot-reload.
        // The validation is split into a pure helper
        // (`validate_refit_counts`) so it's unit-testable without a
        // Vulkan context.
        let built_counts = self
            .skinned_blas
            .get(&entity_id)
            .map(|e| (e.built_vertex_count, e.built_index_count))
            .with_context(|| format!("no skinned BLAS for entity {entity_id}"))?;
        if let Err(mismatch) =
            validate_refit_counts(built_counts.0, built_counts.1, vertex_count, index_count)
        {
            debug_assert!(
                false,
                "BLAS refit count mismatch for entity {entity_id}: {mismatch}"
            );
            log::error!(
                "BLAS refit count mismatch for entity {entity_id}: {mismatch} — \
                 dropping stale BLAS so next frame's first-sight path rebuilds. \
                 Triggered by entity_id → mesh_handle remap (mod swap / LOD switch / \
                 hot-reload?). See #907 / VUID 03667."
            );
            // Drop the entry so the next first-sight loop in `draw.rs`
            // sees `skinned_blas_entry(entity_id).is_none()` and emits
            // a fresh BUILD against the current counts. Borrow on
            // `self.skinned_blas` already released above.
            self.drop_skinned_blas(entity_id);
            return Err(anyhow::anyhow!(
                "refit_skinned_blas: {mismatch} — entry dropped, will rebuild next frame"
            ));
        }
        let entry = self
            .skinned_blas
            .get_mut(&entity_id)
            .with_context(|| format!("no skinned BLAS for entity {entity_id}"))?;
        let scratch_buffer = self.blas_scratch_buffer.as_ref().context(
            "blas_scratch_buffer absent — must be allocated by build_skinned_blas first",
        )?;

        let vertex_stride = std::mem::size_of::<Vertex>() as vk::DeviceSize;
        let vertex_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(vertex_buffer),
            )
        };
        let index_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(index_buffer),
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
        let geometry = vk::AccelerationStructureGeometryKHR::default()
            .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
            .flags(vk::GeometryFlagsKHR::OPAQUE)
            .geometry(vk::AccelerationStructureGeometryDataKHR { triangles });
        let primitive_count = index_count / 3;
        let scratch_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(scratch_buffer.buffer),
            )
        };
        debug_assert!(
            is_scratch_aligned(scratch_address, scratch_align),
            "refit_skinned_blas: scratch device address {scratch_address:#x} is not \
             aligned to minAccelerationStructureScratchOffsetAlignment \
             ({scratch_align}); see #659"
        );

        // mode = UPDATE: src == dst == this entity's BLAS. Vulkan
        // refits in-place against the new vertex data; topology must
        // stay identical to the original BUILD's geometry. The shared
        // `UPDATABLE_AS_FLAGS` constant guarantees this UPDATE's flag
        // set matches the original BUILD (VUID-…-pInfos-03667). See
        // #958 / REN-D8-NEW-14.
        let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
            .flags(UPDATABLE_AS_FLAGS)
            .mode(vk::BuildAccelerationStructureModeKHR::UPDATE)
            .src_acceleration_structure(entry.accel)
            .dst_acceleration_structure(entry.accel)
            .geometries(std::slice::from_ref(&geometry))
            .scratch_data(vk::DeviceOrHostAddressKHR {
                device_address: scratch_address,
            });
        let range_info = vk::AccelerationStructureBuildRangeInfoKHR::default()
            .primitive_count(primitive_count)
            .primitive_offset(0)
            .first_vertex(0);
        unsafe {
            self.accel_loader.cmd_build_acceleration_structures(
                cmd,
                &[build_info],
                &[std::slice::from_ref(&range_info)],
            );
        }
        entry.last_used_frame = self.frame_counter;
        // #679 / AS-8-9 — track refit chain length so the renderer
        // can drop+rebuild the BLAS once the BVH degrades from too
        // many in-place updates. `saturating_add` rather than `+=`
        // is paranoia: a one-frame overflow is harmless (the
        // threshold check still fires) but avoids panicking in
        // debug builds if a hypothetical pathological case keeps
        // refitting an entity for ~136 years at 60 FPS without ever
        // tripping the rebuild gate.
        entry.refit_count = entry.refit_count.saturating_add(1);
        Ok(())
    }

    /// Decide whether the skinned BLAS for `entity_id` has refit
    /// enough times that its BVH quality has degraded and it should
    /// be dropped + rebuilt this frame. Returns `false` for missing
    /// entries (no BLAS = nothing to rebuild) and for skinned BLAS
    /// whose refit count is below [`SKINNED_BLAS_REFIT_THRESHOLD`].
    ///
    /// The caller (typically `draw_frame`) should drop the entry
    /// via [`Self::drop_skinned_blas`] and then re-enter the
    /// first-sight build path so the next frame's
    /// `cmd_build_acceleration_structures(BUILD)` produces a fresh
    /// BVH that tightly fits the current pose. See #679 / AS-8-9.
    pub fn should_rebuild_skinned_blas(&self, entity_id: EntityId) -> bool {
        self.skinned_blas
            .get(&entity_id)
            .map(|entry| should_rebuild_skinned_blas_after(entry.refit_count))
            .unwrap_or(false)
    }

    /// Emit the inter-build scratch-buffer serialise barrier required
    /// when consecutive `cmd_build_acceleration_structures` calls share
    /// the same scratch region (as every BLAS build / refit in this
    /// manager does — `blas_scratch_buffer` is allocated once and
    /// reused). Vulkan spec
    /// (`VkAccelerationStructureBuildGeometryInfoKHR > scratchData`)
    /// requires `ACCELERATION_STRUCTURE_WRITE → ACCELERATION_STRUCTURE_WRITE`
    /// at `ACCELERATION_STRUCTURE_BUILD_KHR` stage between such calls.
    ///
    /// Stateless (the `&self` is for discoverability — the helper does
    /// not touch any field). Caller emits this between iterations of a
    /// build loop, **not** before the first iteration. See #642.
    pub fn record_scratch_serialize_barrier(&self, device: &ash::Device, cmd: vk::CommandBuffer) {
        let barrier = vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR)
            .dst_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR);
        unsafe {
            // SAFETY: `cmd` is a recording command buffer the caller
            // owns; the barrier touches only AS-build state and has
            // no aliasing concerns.
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                vk::DependencyFlags::empty(),
                &[barrier],
                &[],
                &[],
            );
        }
    }

    /// Look up a per-skinned-entity BLAS entry. Used by the TLAS
    /// build path to override the per-mesh BLAS lookup when a
    /// `DrawCommand` carries a `skin_slot_id`.
    pub fn skinned_blas_entry(&self, entity_id: EntityId) -> Option<&BlasEntry> {
        self.skinned_blas.get(&entity_id)
    }

    /// Drop a per-skinned-entity BLAS. Routes through `pending_destroy_blas`
    /// with a `MAX_FRAMES_IN_FLIGHT`-frame countdown so the acceleration
    /// structure is never destroyed while a command buffer still references
    /// it. Mirrors `drop_blas`; `tick_deferred_destroy` and `destroy`
    /// both drain the queue.
    pub fn drop_skinned_blas(&mut self, entity_id: EntityId) {
        if let Some(entry) = self.skinned_blas.remove(&entity_id) {
            // Skinned BLAS aren't tracked in `static_blas_bytes` (see
            // counterpart in `build_skinned_blas`), so only the total
            // counter decrements here.
            self.total_blas_bytes = self.total_blas_bytes.saturating_sub(entry.size_bytes);
            self.pending_destroy_blas
                .push(entry, MAX_FRAMES_IN_FLIGHT as u32);
            self.blas_map_generation = self.blas_map_generation.wrapping_add(1);
        }
    }

    /// Iterator over all skinned-entity IDs the manager currently
    /// holds a BLAS for. Used by the renderer's Drop chain to
    /// collect IDs for bulk teardown without holding a reference
    /// across mutation.
    pub fn skinned_blas_entities(&self) -> Vec<EntityId> {
        self.skinned_blas.keys().copied().collect()
    }
}
