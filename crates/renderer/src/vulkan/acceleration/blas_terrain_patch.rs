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

/// The op for each request, in request order, into `ops` (emptied first). `previous` is what each
/// mesh's BLAS was last built or refit from, with its refit count.
pub(super) fn plan_patch_blas_ops(
    requests: &[(Option<(TerrainPatchBlasContent, u32)>, TerrainPatchBlasContent)],
    build_budget: usize,
    rebuild_after: u32,
    ops: &mut Vec<PatchBlasOp>,
) {
    let mut builds = 0;
    ops.clear();
    ops.extend(requests
        .iter()
        .map(|&(previous, now)| match previous {
            Some((built, refits)) if built == now && refits < rebuild_after => PatchBlasOp::Refit,
            _ if builds < build_budget => {
                builds += 1;
                PatchBlasOp::Build
            }
            _ => PatchBlasOp::Defer,
        }));
}

/// `buffer`, emptied, as a `Vec` of another element type. Std's in-place `collect` keeps the allocation when
/// the two share a layout, which is how the record path keeps a list whose elements borrow one frame's data
/// (the build infos, the range slices) across frames without reallocating it (#7246).
fn emptied<T, U>(mut buffer: Vec<T>) -> Vec<U> {
    buffer.clear();
    buffer.into_iter().map(|_| unreachable!("the buffer was cleared")).collect()
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
    /// The record path's per-frame working set (#7246).
    frame: PatchBlasFrame,
    /// What each entry's `content` held before this frame's record, until the frame's submit settles (#7360).
    staged: StagedPatchContent,
}

/// The content each patch BLAS held before the frame being recorded wrote its build or refit (#7360).
///
/// `entry.content` is the planner's only input for "refit or build", and a record writes it as soon as it
/// has recorded the op. That is a submit-time fact: when one of `draw_frame`'s tail `Err` sites discards the
/// command buffer, a first-time BUILD never ran, and the next frame would plan an UPDATE refit of an
/// acceleration structure that was never built (the skinned set's #3991). The record stages the prior
/// value here; [`Self::commit`] (after `queue_submit` returns `Ok`) forgets it and [`Self::rollback`]
/// (a discarded command buffer) hands it back so the next frame plans from what the GPU really holds.
#[derive(Default)]
struct StagedPatchContent {
    prior: Vec<(u32, Option<(TerrainPatchBlasContent, u32)>)>,
}

impl StagedPatchContent {
    /// A new frame's record starts from an empty stage: a stage the previous frame never settled (it
    /// returned before the submit) cannot be undone any more, and holding it would restore stale content.
    fn begin_frame(&mut self) {
        self.prior.clear();
    }

    fn stage(&mut self, mesh: u32, prior: Option<(TerrainPatchBlasContent, u32)>) {
        self.prior.push((mesh, prior));
    }

    /// The frame was submitted: what it recorded is the new truth.
    fn commit(&mut self) {
        self.prior.clear();
    }

    /// The frame was discarded: every staged entry's content, newest first so a mesh staged twice ends on
    /// its oldest value.
    fn rollback(&mut self) -> impl Iterator<Item = (u32, Option<(TerrainPatchBlasContent, u32)>)> + '_ {
        self.prior.drain(..).rev()
    }
}

/// The patch BLAS record path's per-frame lists, refilled every frame and never freed, so once the first exterior
/// frame has sized them a frame with resident patches allocates nothing. Each holds at most one entry per
/// dispatched patch slot (`TERRAIN_PATCH_SLOTS`), so they need no shrink policy.
#[derive(Default)]
struct PatchBlasFrame {
    requests: Vec<TerrainPatchBlasRequest>,
    /// Indices into `requests` that passed validation.
    live: Vec<usize>,
    plan: Vec<(Option<(TerrainPatchBlasContent, u32)>, TerrainPatchBlasContent)>,
    ops: Vec<PatchBlasOp>,
    regions: Vec<vk::DeviceSize>,
    /// (index into `live`, op) for every op not deferred.
    recorded: Vec<(usize, PatchBlasOp)>,
    geometries: Vec<vk::AccelerationStructureGeometryKHR<'static>>,
    ranges: Vec<vk::AccelerationStructureBuildRangeInfoKHR>,
    /// Always empty between frames: kept only for its allocation (see [`emptied`]).
    infos: Vec<vk::AccelerationStructureBuildGeometryInfoKHR<'static>>,
    range_refs: Vec<&'static [vk::AccelerationStructureBuildRangeInfoKHR]>,
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

    /// This frame's patch BLAS request list, emptied, for the patch pass to fill before
    /// [`Self::record_terrain_patch_blas`] reads it. The buffer is kept across frames (#7246).
    pub fn terrain_patch_blas_requests(&mut self) -> &mut Vec<TerrainPatchBlasRequest> {
        let requests = &mut self.terrain_patch_blas.frame.requests;
        requests.clear();
        requests
    }

    /// Build or refit this frame's patch BLAS from the vertices the patch
    /// compute pass just wrote (the requests filled through
    /// [`Self::terrain_patch_blas_requests`]), and mark which may enter the TLAS.
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
    ) -> TerrainPatchBlasStats {
        // The working set leaves the set for the record so its vectors and the
        // manager can be borrowed apart; it goes back whatever the outcome.
        let mut frame = std::mem::take(&mut self.terrain_patch_blas.frame);
        // SAFETY: the caller's contract, passed through.
        let stats = unsafe { self.record_patch_blas_frame(device, allocator, cmd, &mut frame) };
        self.terrain_patch_blas.frame = frame;
        stats
    }

    /// [`Self::record_terrain_patch_blas`]'s body, over the reused working set.
    ///
    /// # Safety
    /// As [`Self::record_terrain_patch_blas`].
    unsafe fn record_patch_blas_frame(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        cmd: vk::CommandBuffer,
        frame: &mut PatchBlasFrame,
    ) -> TerrainPatchBlasStats {
        let PatchBlasFrame { requests, live, plan, ops, regions, recorded, geometries, ranges, infos, range_refs } =
            frame;
        self.terrain_patch_blas.ready.clear();
        self.terrain_patch_blas.staged.begin_frame();
        let mut stats = TerrainPatchBlasStats::default();

        // Size every first-seen mesh, and drop requests whose entry cannot
        // exist or whose counts no longer match it (VUID-03667: an UPDATE, and
        // a BUILD into an AS sized for other counts, must not happen). `live`
        // holds indices into `requests`.
        live.clear();
        for (index, request) in requests.iter().enumerate() {
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
            live.push(index);
        }

        plan.clear();
        plan.extend(live.iter().map(|&i| {
            let request = &requests[i];
            (self.terrain_patch_blas.entries[&request.mesh].content, request.content)
        }));
        plan_patch_blas_ops(
            plan,
            MAX_TERRAIN_PATCH_BLAS_BUILDS_PER_FRAME,
            TERRAIN_PATCH_BLAS_REBUILD_AFTER_REFITS,
            ops,
        );

        // Disjoint, aligned scratch regions, one per recorded op.
        let align = self.scratch_align;
        regions.clear();
        let mut total: vk::DeviceSize = 0;
        for (&i, op) in live.iter().zip(ops.iter()) {
            let entry = &self.terrain_patch_blas.entries[&requests[i].mesh];
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
        recorded.clear();
        geometries.clear();
        ranges.clear();
        for (slot, (&i, op)) in live.iter().zip(ops.iter()).enumerate() {
            if *op == PatchBlasOp::Defer {
                continue;
            }
            let request = &requests[i];
            recorded.push((slot, *op));
            geometries.push(patch_geometry(&request.content, request.vertex_count));
            ranges.push(vk::AccelerationStructureBuildRangeInfoKHR {
                primitive_count: request.index_count / 3,
                primitive_offset: 0,
                first_vertex: 0,
                transform_offset: 0,
            });
        }
        // The two lists that borrow this frame's geometries and ranges reuse
        // their kept allocations through `emptied`.
        let mut frame_infos: Vec<vk::AccelerationStructureBuildGeometryInfoKHR<'_>> =
            emptied(std::mem::take(infos));
        frame_infos.extend(recorded.iter().zip(geometries.iter()).map(|(&(slot, op), geometry)| {
            let entry = &self.terrain_patch_blas.entries[&requests[live[slot]].mesh];
            let info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                .flags(TERRAIN_PATCH_BLAS_FLAGS)
                .dst_acceleration_structure(entry.accel)
                .geometries(std::slice::from_ref(geometry))
                .scratch_data(vk::DeviceOrHostAddressKHR {
                    device_address: scratch_base + regions[slot],
                });
            match op {
                PatchBlasOp::Refit => info
                    .mode(vk::BuildAccelerationStructureModeKHR::UPDATE)
                    .src_acceleration_structure(entry.accel),
                _ => info.mode(vk::BuildAccelerationStructureModeKHR::BUILD),
            }
        }));
        let mut frame_range_refs: Vec<&[vk::AccelerationStructureBuildRangeInfoKHR]> =
            emptied(std::mem::take(range_refs));
        frame_range_refs.extend(ranges.iter().map(std::slice::from_ref));

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
                .cmd_build_acceleration_structures(cmd, &frame_infos, &frame_range_refs);
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
        *infos = emptied(frame_infos);
        *range_refs = emptied(frame_range_refs);

        for &(slot, op) in recorded.iter() {
            let request = &requests[live[slot]];
            let entry = self
                .terrain_patch_blas
                .entries
                .get_mut(&request.mesh)
                .expect("entry checked above");
            self.terrain_patch_blas.staged.stage(request.mesh, entry.content);
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

    /// The frame's command buffer was submitted: its patch BLAS builds and refits are what the GPU holds
    /// (#7360). Called beside `commit_provisional_skinned_blas`, after `queue_submit` returns `Ok`.
    pub fn commit_terrain_patch_blas(&mut self) {
        self.terrain_patch_blas.staged.commit();
    }

    /// The frame's command buffer was discarded: put every patch BLAS's content back to what it held before
    /// the record, so the next frame builds (rather than UPDATE-refits) an AS whose build never ran (#7360).
    /// Returns how many entries were restored.
    pub fn rollback_terrain_patch_blas(&mut self) -> usize {
        let set = &mut self.terrain_patch_blas;
        let mut restored = 0;
        for (mesh, prior) in set.staged.rollback() {
            if let Some(entry) = set.entries.get_mut(&mesh) {
                entry.content = prior;
                restored += 1;
            }
        }
        // None of this frame's BLAS is a usable answer for the TLAS any more.
        set.ready.clear();
        restored
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
        self.terrain_patch_blas.staged.commit();
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

    fn plan(
        requests: &[(Option<(TerrainPatchBlasContent, u32)>, TerrainPatchBlasContent)],
        build_budget: usize,
        rebuild_after: u32,
    ) -> Vec<PatchBlasOp> {
        let mut ops = vec![PatchBlasOp::Defer; 7];
        plan_patch_blas_ops(requests, build_budget, rebuild_after, &mut ops);
        ops
    }

    /// #7246 — the frame lists keep their allocation: the planner refills the vector it is handed, and `emptied`
    /// carries a buffer's capacity over to a list whose elements borrow another frame's data.
    #[test]
    fn the_record_paths_frame_lists_keep_their_allocations() {
        let a = content(1);
        let mut ops = Vec::with_capacity(16);
        let before = ops.as_ptr();
        plan_patch_blas_ops(&[(Some((a, 0)), a), (None, content(2))], 2, 600, &mut ops);
        assert_eq!(ops, vec![PatchBlasOp::Refit, PatchBlasOp::Build]);
        assert_eq!(ops.as_ptr(), before, "the planner refills the vector it is handed");

        let kept: Vec<vk::AccelerationStructureBuildGeometryInfoKHR<'static>> = Vec::with_capacity(24);
        let geometry = vk::AccelerationStructureGeometryKHR::default();
        let mut infos: Vec<vk::AccelerationStructureBuildGeometryInfoKHR<'_>> = emptied(kept);
        assert!(infos.capacity() >= 24, "the borrowed list reuses the kept allocation");
        infos.push(vk::AccelerationStructureBuildGeometryInfoKHR::default().geometries(std::slice::from_ref(&geometry)));
        let back: Vec<vk::AccelerationStructureBuildGeometryInfoKHR<'static>> = emptied(infos);
        assert!(back.is_empty() && back.capacity() >= 24, "and hands it back, empty, for the next frame");
    }

    #[test]
    fn the_same_square_refits_and_a_new_one_builds_within_the_budget() {
        let a = content(1);
        let ops = plan(
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

    /// #7360 — a discarded frame hands back the content each BLAS held before its record, so a first-time
    /// BUILD that never ran is planned as a BUILD again, never an UPDATE refit of an unbuilt AS.
    #[test]
    fn a_discarded_frames_first_build_is_planned_as_a_build_again() {
        let (a, b) = (content(1), content(2));
        // Mesh 10 was never built; mesh 11 held square `a` (3 refits) and is reassigned to `b`.
        let mut staged = StagedPatchContent::default();
        staged.begin_frame();
        staged.stage(10, None);
        staged.stage(11, Some((a, 3)));
        let mut contents: std::collections::HashMap<u32, Option<(TerrainPatchBlasContent, u32)>> =
            [(10, Some((a, 0))), (11, Some((b, 0)))].into_iter().collect();

        // Unrolled, the record's commit makes the next frame refit both.
        assert_eq!(plan(&[(contents[&10], a), (contents[&11], b)], 4, 600), vec![PatchBlasOp::Refit; 2]);

        for (mesh, prior) in staged.rollback() {
            contents.insert(mesh, prior);
        }
        assert_eq!(contents[&10], None);
        assert_eq!(contents[&11], Some((a, 3)));
        assert_eq!(
            plan(&[(contents[&10], a), (contents[&11], b)], 4, 600),
            vec![PatchBlasOp::Build, PatchBlasOp::Build],
            "an unbuilt AS builds again, and a reassigned one rebuilds for its new square"
        );
        assert_eq!(staged.rollback().count(), 0, "a rollback settles the stage");
    }

    #[test]
    fn a_submitted_frame_forgets_its_stage_and_a_new_record_starts_empty() {
        let mut staged = StagedPatchContent::default();
        staged.stage(1, None);
        staged.commit();
        assert_eq!(staged.rollback().count(), 0, "a committed frame has nothing to undo");
        staged.stage(2, None);
        staged.begin_frame();
        assert_eq!(staged.rollback().count(), 0, "an unsettled stage is not carried into the next record");
        // A mesh staged twice in one frame ends on its oldest value.
        let a = content(1);
        staged.stage(3, None);
        staged.stage(3, Some((a, 1)));
        let restored: Vec<_> = staged.rollback().collect();
        assert_eq!(restored.last(), Some(&(3, None)));
    }

    #[test]
    fn a_moved_pool_or_the_refit_limit_forces_a_build() {
        let a = content(1);
        let moved = TerrainPatchBlasContent { vertex_address: 0x9000, ..a };
        let ops = plan(&[(Some((a, 0)), moved), (Some((a, 600)), a)], 4, 600);
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
