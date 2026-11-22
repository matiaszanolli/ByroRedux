//! TSD-4.2 terrain displacement patch BLAS (#7221).
//!
//! Design: `docs/engine/terrain-displacement.md` §3.5. Each reserved patch mesh
//! (24 slots × fine/coarse, `vulkan/terrain_patch.rs`) gets its own bottom-level
//! AS over the bytes `terrain_patch.comp` rewrites in the global vertex pool, so
//! a patch hit-shades like any rigid instance: `ray_hit.glsl` resolves every hit
//! through `GpuInstance` + primitive id and never reads a geometry index, which
//! rules out one multi-geometry BLAS without touching every hit path.
//!
//! Per frame, after the patch compute and before the TLAS build:
//! - a patch whose mesh shows the same square as last frame is **refit**
//!   (only the distance fade moved its vertices);
//! - a patch whose mesh changed square, or was never built, or has been refit
//!   [`TERRAIN_PATCH_BLAS_REBUILD_AFTER_REFITS`] times, is **built** — at most
//!   [`MAX_TERRAIN_PATCH_BLAS_BUILDS_PER_FRAME`] a frame (§3.5);
//! - the rest are **deferred**: their BLAS still describes another square, so
//!   they stay out of the TLAS this frame and rays see the base terrain there.
//!
//! All of a frame's builds and refits go out in one
//! `vkCmdBuildAccelerationStructuresKHR` call over disjoint regions of this
//! set's own scratch buffer, so they do not pay the per-BLAS serialize barrier
//! the shared-scratch skinned refit does (§8.1: that barrier is most of a
//! refit's fixed cost).
//!
//! The AS objects live for the session (48 at most, sized once per mesh) and go
//! at teardown; only a grown scratch buffer is retired through the deferred
//! destroy queue.

use super::predicates::{align_scratch_address, scratch_alignment_padding};
use super::AccelerationManager;
use crate::deferred_destroy::DEFAULT_COUNTDOWN;
use crate::vulkan::allocator::SharedAllocator;
use crate::vulkan::buffer::GpuBuffer;
use anyhow::{Context, Result};
use ash::vk;

/// Refit every frame, rebuilt on reassignment: the skinned-BLAS flag set.
pub(super) const TERRAIN_PATCH_BLAS_FLAGS: vk::BuildAccelerationStructureFlagsKHR =
    vk::BuildAccelerationStructureFlagsKHR::from_raw(
        vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_BUILD.as_raw()
            | vk::BuildAccelerationStructureFlagsKHR::ALLOW_UPDATE.as_raw(),
    );
/// §3.5: at most two full patch builds a frame; the rest wait their turn.
pub const MAX_TERRAIN_PATCH_BLAS_BUILDS_PER_FRAME: usize = 2;
/// Refits before a fresh build restores BVH quality, as for skinned BLAS.
pub const TERRAIN_PATCH_BLAS_REBUILD_AFTER_REFITS: u32 = 600;

/// What a patch mesh's vertices describe this frame. A BLAS refits only
/// against the content it was built from; anything else needs a build.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TerrainPatchBlasContent {
    /// The square the mesh shows (the host's slot assignment, hashed).
    pub square: u64,
    /// Device addresses of the mesh's first vertex and first index. They
    /// move when the global geometry pool is rebuilt or compacted.
    pub vertex_address: vk::DeviceAddress,
    pub index_address: vk::DeviceAddress,
}

/// One patch mesh to build or refit this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainPatchBlasRequest {
    pub mesh: u32,
    pub content: TerrainPatchBlasContent,
    pub vertex_count: u32,
    pub index_count: u32,
}

/// The last frame's patch BLAS work, for `terrain.patches`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerrainPatchBlasStats {
    /// Patch BLAS the TLAS may use this frame.
    pub ready: u32,
    pub built: u32,
    pub refit: u32,
    /// Waiting for the build budget; out of the TLAS this frame.
    pub deferred: u32,
    /// AS creation or sizing failed; out of the TLAS.
    pub failed: u32,
    /// Bytes held by every patch BLAS created so far.
    pub bytes: u64,
}

/// What this frame does with one requested patch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PatchBlasOp {
    Build,
    Refit,
    Defer,
}

/// The op for each request, in request order. `previous` is what each
/// mesh's BLAS was last built or refit from, with its refit count.
pub(super) fn plan_patch_blas_ops(
    requests: &[(Option<(TerrainPatchBlasContent, u32)>, TerrainPatchBlasContent)],
    build_budget: usize,
    rebuild_after: u32,
) -> Vec<PatchBlasOp> {
    let mut builds = 0;
    requests
        .iter()
        .map(|&(previous, now)| match previous {
            Some((built, refits)) if built == now && refits < rebuild_after => PatchBlasOp::Refit,
            _ if builds < build_budget => {
                builds += 1;
                PatchBlasOp::Build
            }
            _ => PatchBlasOp::Defer,
        })
        .collect()
}

/// Round `size` up to a multiple of `align` (≥ 1).
fn align_size(size: vk::DeviceSize, align: u32) -> vk::DeviceSize {
    let a = vk::DeviceSize::from(align.max(1));
    size.div_ceil(a) * a
}

pub(super) struct PatchBlasEntry {
    accel: vk::AccelerationStructureKHR,
    buffer: GpuBuffer,
    device_address: vk::DeviceAddress,
    vertex_count: u32,
    index_count: u32,
    build_scratch: vk::DeviceSize,
    update_scratch: vk::DeviceSize,
    /// What the AS holds and its refits since the last build; `None` until
    /// a build is recorded.
    content: Option<(TerrainPatchBlasContent, u32)>,
}

/// Every patch BLAS, owned by the [`AccelerationManager`].
#[derive(Default)]
pub struct TerrainPatchBlasSet {
    /// The reserved patch mesh handles (sorted).
    meshes: Vec<u32>,
    entries: rustc_hash::FxHashMap<u32, PatchBlasEntry>,
    scratch: Option<GpuBuffer>,
    /// Meshes whose BLAS describes this frame's vertices.
    ready: rustc_hash::FxHashSet<u32>,
    stats: TerrainPatchBlasStats,
}

impl TerrainPatchBlasSet {
    pub(super) fn is_patch_mesh(&self, mesh: u32) -> bool {
        self.meshes.binary_search(&mesh).is_ok()
    }

    /// The BLAS address for a patch draw, when its BLAS matches this frame.
    pub(super) fn ready_address(&self, mesh: u32) -> Option<vk::DeviceAddress> {
        self.ready
            .contains(&mesh)
            .then(|| self.entries.get(&mesh).map(|e| e.device_address))
            .flatten()
    }

    fn bytes(&self) -> u64 {
        self.entries.values().map(|e| e.buffer.size).sum()
    }
}

impl AccelerationManager {
    /// Declare the reserved patch meshes (once, after the renderer reserves
    /// them). Their draws then take a patch BLAS in the TLAS, never a static
    /// one: the static restore path skips them (it would snapshot vertices the
    /// compute pass rewrites every frame).
    pub fn register_terrain_patch_meshes(&mut self, meshes: &[u32]) {
        let mut sorted = meshes.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        self.terrain_patch_blas.meshes = sorted;
    }

    /// Whether `mesh` is a reserved terrain patch mesh.
    pub fn is_terrain_patch_mesh(&self, mesh: u32) -> bool {
        self.terrain_patch_blas.is_patch_mesh(mesh)
    }

    /// The last [`Self::record_terrain_patch_blas`]'s counts.
    pub fn terrain_patch_blas_stats(&self) -> TerrainPatchBlasStats {
        self.terrain_patch_blas.stats
    }

    /// Size and create the AS for a patch mesh's first request.
    fn create_patch_blas_entry(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        request: &TerrainPatchBlasRequest,
    ) -> Result<()> {
        let geometry = patch_geometry(&request.content, request.vertex_count);
        let size_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
            .flags(TERRAIN_PATCH_BLAS_FLAGS)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .geometries(std::slice::from_ref(&geometry));
        let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
        // SAFETY: the geometry describes a triangle list in live buffers; a
        // size query reads no device memory.
        unsafe {
            self.accel_loader.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &size_info,
                &[request.index_count / 3],
                &mut sizes,
            );
        }
        let mut buffer = GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            sizes.acceleration_structure_size,
            vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        )
        .context("allocate terrain patch BLAS")?;
        let create_info = vk::AccelerationStructureCreateInfoKHR::default()
            .buffer(buffer.buffer)
            .size(sizes.acceleration_structure_size)
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL);
        // SAFETY: `buffer` was created with AS storage usage and is at least
        // the queried size.
        let accel = match unsafe { self.accel_loader.create_acceleration_structure(&create_info, None) } {
            Ok(accel) => accel,
            Err(e) => {
                buffer.destroy(device, allocator);
                return Err(anyhow::anyhow!("create terrain patch BLAS: {e}"));
            }
        };
        // SAFETY: `accel` was just created.
        let device_address = unsafe {
            self.accel_loader.get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                    .acceleration_structure(accel),
            )
        };
        self.total_blas_bytes += buffer.size;
        self.terrain_patch_blas.entries.insert(
            request.mesh,
            PatchBlasEntry {
                accel,
                buffer,
                device_address,
                vertex_count: request.vertex_count,
                index_count: request.index_count,
                build_scratch: sizes.build_scratch_size,
                update_scratch: sizes.update_scratch_size,
                content: None,
            },
        );
        // A new BLAS address can appear in the TLAS instance list.
        self.blas_map_generation = self.blas_map_generation.wrapping_add(1);
        Ok(())
    }

    /// Build or refit this frame's patch BLAS from the vertices the patch
    /// compute pass just wrote, and mark which may enter the TLAS.
    ///
    /// # Safety
    /// `cmd` must be recording outside a render pass, after the patch compute
    /// dispatches and a barrier making their writes visible to
    /// `ACCELERATION_STRUCTURE_BUILD` shader reads, and before this frame's
    /// TLAS build. Every request's addresses must point into the live global
    /// vertex / index buffers.
    pub unsafe fn record_terrain_patch_blas(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        cmd: vk::CommandBuffer,
        requests: &[TerrainPatchBlasRequest],
    ) -> TerrainPatchBlasStats {
        self.terrain_patch_blas.ready.clear();
        let mut stats = TerrainPatchBlasStats::default();

        // Size every first-seen mesh, and drop requests whose entry cannot
        // exist or whose counts no longer match it (VUID-03667: an UPDATE, and
        // a BUILD into an AS sized for other counts, must not happen).
        let mut live: Vec<&TerrainPatchBlasRequest> = Vec::with_capacity(requests.len());
        for request in requests {
            if !self.terrain_patch_blas.is_patch_mesh(request.mesh) || request.index_count < 3 {
                stats.failed += 1;
                continue;
            }
            if !self.terrain_patch_blas.entries.contains_key(&request.mesh)
                && let Err(e) = self.create_patch_blas_entry(device, allocator, request)
            {
                log::warn!("terrain patch BLAS for mesh {}: {e:#}", request.mesh);
                stats.failed += 1;
                continue;
            }
            let entry = &self.terrain_patch_blas.entries[&request.mesh];
            if entry.vertex_count != request.vertex_count || entry.index_count != request.index_count {
                debug_assert!(false, "terrain patch mesh {} changed size", request.mesh);
                stats.failed += 1;
                continue;
            }
            live.push(request);
        }

        let plan_input: Vec<_> = live
            .iter()
            .map(|r| (self.terrain_patch_blas.entries[&r.mesh].content, r.content))
            .collect();
        let ops = plan_patch_blas_ops(
            &plan_input,
            MAX_TERRAIN_PATCH_BLAS_BUILDS_PER_FRAME,
            TERRAIN_PATCH_BLAS_REBUILD_AFTER_REFITS,
        );

        // Disjoint, aligned scratch regions, one per recorded op.
        let align = self.scratch_align;
        let mut regions: Vec<vk::DeviceSize> = Vec::with_capacity(live.len());
        let mut total: vk::DeviceSize = 0;
        for (request, op) in live.iter().zip(&ops) {
            let entry = &self.terrain_patch_blas.entries[&request.mesh];
            let need = match op {
                PatchBlasOp::Build => entry.build_scratch,
                PatchBlasOp::Refit => entry.update_scratch,
                PatchBlasOp::Defer => 0,
            };
            regions.push(total);
            total += align_size(need, align);
        }
        stats.deferred = ops.iter().filter(|op| **op == PatchBlasOp::Defer).count() as u32;
        if total == 0 {
            stats.bytes = self.terrain_patch_blas.bytes();
            self.terrain_patch_blas.stats = stats;
            return stats;
        }
        let scratch_size = total + scratch_alignment_padding(align);
        if self.terrain_patch_blas.scratch.as_ref().is_none_or(|s| s.size < scratch_size) {
            match GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                scratch_size,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            ) {
                Ok(grown) => {
                    // The old buffer may still be in flight: retire it like
                    // every other scratch buffer (#1782).
                    if let Some(old) = self.terrain_patch_blas.scratch.replace(grown) {
                        self.pending_destroy_scratch.push(old, DEFAULT_COUNTDOWN);
                    }
                }
                Err(e) => {
                    log::warn!("terrain patch BLAS scratch ({scratch_size} B): {e:#}");
                    stats.failed += live.len() as u32 - stats.deferred;
                    stats.bytes = self.terrain_patch_blas.bytes();
                    self.terrain_patch_blas.stats = stats;
                    return stats;
                }
            }
        }
        let scratch = self.terrain_patch_blas.scratch.as_ref().expect("sized above");
        // SAFETY: the scratch buffer has SHADER_DEVICE_ADDRESS usage.
        let scratch_base = align_scratch_address(
            unsafe {
                device.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default().buffer(scratch.buffer),
                )
            },
            align,
        );

        // Geometries first: the build infos borrow them.
        let mut recorded: Vec<(usize, PatchBlasOp)> = Vec::with_capacity(live.len());
        let mut geometries: Vec<vk::AccelerationStructureGeometryKHR<'static>> =
            Vec::with_capacity(live.len());
        let mut ranges: Vec<vk::AccelerationStructureBuildRangeInfoKHR> = Vec::with_capacity(live.len());
        for (i, (request, op)) in live.iter().zip(&ops).enumerate() {
            if *op == PatchBlasOp::Defer {
                continue;
            }
            recorded.push((i, *op));
            geometries.push(patch_geometry(&request.content, request.vertex_count));
            ranges.push(vk::AccelerationStructureBuildRangeInfoKHR {
                primitive_count: request.index_count / 3,
                primitive_offset: 0,
                first_vertex: 0,
                transform_offset: 0,
            });
        }
        let infos: Vec<vk::AccelerationStructureBuildGeometryInfoKHR<'_>> = recorded
            .iter()
            .zip(&geometries)
            .map(|(&(i, op), geometry)| {
                let entry = &self.terrain_patch_blas.entries[&live[i].mesh];
                let info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                    .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                    .flags(TERRAIN_PATCH_BLAS_FLAGS)
                    .dst_acceleration_structure(entry.accel)
                    .geometries(std::slice::from_ref(geometry))
                    .scratch_data(vk::DeviceOrHostAddressKHR {
                        device_address: scratch_base + regions[i],
                    });
                match op {
                    PatchBlasOp::Refit => info
                        .mode(vk::BuildAccelerationStructureModeKHR::UPDATE)
                        .src_acceleration_structure(entry.accel),
                    _ => info.mode(vk::BuildAccelerationStructureModeKHR::BUILD),
                }
            })
            .collect();
        let range_refs: Vec<&[vk::AccelerationStructureBuildRangeInfoKHR]> =
            ranges.iter().map(std::slice::from_ref).collect();

        // SAFETY: caller contract — `cmd` is recording outside a render pass
        // after the patch compute and its barrier. Each info writes its own
        // AS through its own scratch region; an UPDATE reads only its own AS.
        unsafe {
            // Before: last frame's builds of these AS and their scratch (same
            // queue, earlier submission) must finish before this frame's
            // builds reuse the scratch and update the AS in place.
            crate::vulkan::descriptors::memory_barrier(
                device,
                cmd,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR
                    | vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
            );
            self.accel_loader
                .cmd_build_acceleration_structures(cmd, &infos, &range_refs);
            // After: the TLAS build and every ray query read the results.
            crate::vulkan::descriptors::memory_barrier(
                device,
                cmd,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR,
            );
        }
        drop(infos);

        for &(i, op) in &recorded {
            let request = live[i];
            let entry = self
                .terrain_patch_blas
                .entries
                .get_mut(&request.mesh)
                .expect("entry checked above");
            entry.content = Some(match op {
                PatchBlasOp::Refit => {
                    stats.refit += 1;
                    let (content, refits) = entry.content.expect("a refit has a built content");
                    (content, refits + 1)
                }
                _ => {
                    stats.built += 1;
                    (request.content, 0)
                }
            });
            self.terrain_patch_blas.ready.insert(request.mesh);
        }
        stats.ready = self.terrain_patch_blas.ready.len() as u32;
        stats.bytes = self.terrain_patch_blas.bytes();
        self.terrain_patch_blas.stats = stats;
        stats
    }

    /// Free every patch BLAS and the scratch buffer (teardown, device idle).
    pub(super) unsafe fn destroy_terrain_patch_blas(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
    ) {
        for (_, mut entry) in self.terrain_patch_blas.entries.drain() {
            // SAFETY: teardown runs after device idle; nothing references it.
            unsafe { self.accel_loader.destroy_acceleration_structure(entry.accel, None) };
            self.total_blas_bytes = self.total_blas_bytes.saturating_sub(entry.buffer.size);
            entry.buffer.destroy(device, allocator);
        }
        if let Some(mut scratch) = self.terrain_patch_blas.scratch.take() {
            scratch.destroy(device, allocator);
        }
        self.terrain_patch_blas.ready.clear();
    }
}

/// The patch mesh's triangle list: positions are the first 12 bytes of each
/// 104-byte `Vertex` in the global pool, indices mesh-local `u32`s.
fn patch_geometry(
    content: &TerrainPatchBlasContent,
    vertex_count: u32,
) -> vk::AccelerationStructureGeometryKHR<'static> {
    let triangles = vk::AccelerationStructureGeometryTrianglesDataKHR::default()
        .vertex_format(vk::Format::R32G32B32_SFLOAT)
        .vertex_data(vk::DeviceOrHostAddressConstKHR {
            device_address: content.vertex_address,
        })
        .vertex_stride(std::mem::size_of::<crate::vertex::Vertex>() as vk::DeviceSize)
        .max_vertex(vertex_count.saturating_sub(1))
        .index_type(vk::IndexType::UINT32)
        .index_data(vk::DeviceOrHostAddressConstKHR {
            device_address: content.index_address,
        });
    vk::AccelerationStructureGeometryKHR::default()
        .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
        .flags(vk::GeometryFlagsKHR::OPAQUE)
        .geometry(vk::AccelerationStructureGeometryDataKHR { triangles })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content(square: u64) -> TerrainPatchBlasContent {
        TerrainPatchBlasContent { square, vertex_address: 0x1000, index_address: 0x2000 }
    }

    #[test]
    fn the_same_square_refits_and_a_new_one_builds_within_the_budget() {
        let a = content(1);
        let ops = plan_patch_blas_ops(
            &[
                (Some((a, 3)), a),            // unchanged: refit
                (None, content(2)),           // never built: build
                (Some((a, 0)), content(3)),   // reassigned: build
                (Some((a, 0)), content(4)),   // over budget: defer
                (Some((content(5), 9)), content(5)), // unchanged: refit
            ],
            2,
            600,
        );
        use PatchBlasOp::*;
        assert_eq!(ops, vec![Refit, Build, Build, Defer, Refit]);
    }

    #[test]
    fn a_moved_pool_or_the_refit_limit_forces_a_build() {
        let a = content(1);
        let moved = TerrainPatchBlasContent { vertex_address: 0x9000, ..a };
        let ops = plan_patch_blas_ops(&[(Some((a, 0)), moved), (Some((a, 600)), a)], 4, 600);
        assert_eq!(ops, vec![PatchBlasOp::Build, PatchBlasOp::Build]);
    }

    #[test]
    fn scratch_regions_round_up_to_the_alignment() {
        assert_eq!(align_size(0, 256), 0);
        assert_eq!(align_size(1, 256), 256);
        assert_eq!(align_size(256, 256), 256);
        assert_eq!(align_size(257, 256), 512);
        assert_eq!(align_size(7, 0), 7);
    }

    #[test]
    fn the_flags_allow_refit() {
        assert!(TERRAIN_PATCH_BLAS_FLAGS.contains(vk::BuildAccelerationStructureFlagsKHR::ALLOW_UPDATE));
    }
}
