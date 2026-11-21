//! TSD-4.1 terrain displacement patches (#7152).
//!
//! Design: `docs/engine/terrain-displacement.md` §3. A fixed pool of
//! [`TERRAIN_PATCH_SLOTS`] patch meshes is reserved once in the global vertex
//! pool ([`TerrainPatchPass::reserve_meshes`], global-pool-only uploads: no
//! per-mesh buffers, no BLAS). Every frame the host hands over the resident
//! slots ([`TerrainPatchSlot`]) and the shared parameters
//! ([`TerrainPatchParams`]); [`TerrainPatchPass::record`] re-displaces each
//! resident slot in place with `terrain_patch.comp`, between the instance upload
//! and the main render pass. The raster path draws the patches from the same
//! bytes (the host clones the base cell's draw command onto the patch mesh with
//! `in_tlas = false`); they do not enter the TLAS until stage 4.2.
//!
//! The pass also reads back every slot's border vertices and its largest
//! displacement, one frame-in-flight later, for the `terrain.patches` console
//! check ([`summarize_patch_readback`]).

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::descriptors::{compute_binding, write_storage_buffer, DescriptorPoolBuilder};
use super::reflect::{validate_set_layout, ReflectedShader};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use crate::mesh::MeshRegistry;
use crate::shader_constants::{
    TERRAIN_PATCH_EDGE_SAMPLES, TERRAIN_PATCH_LATTICE_STEP, TERRAIN_PATCH_QUADS,
    TERRAIN_PATCH_READBACK_WORDS, TERRAIN_PATCH_SLOTS, TERRAIN_PATCH_VERTEX_COUNT,
    TERRAIN_PATCH_VERTS, TERRAIN_PATCH_WORKGROUP,
};
use crate::vertex::Vertex;
use anyhow::{Context, Result};
use ash::vk;

pub(super) const TERRAIN_PATCH_COMP_SPV: &[u8] =
    include_bytes!("../../shaders/terrain_patch.comp.spv");

/// Bytes of one frame's readback buffer.
const READBACK_BYTES: u64 =
    TERRAIN_PATCH_SLOTS as u64 * TERRAIN_PATCH_READBACK_WORDS as u64 * 4;

/// One resident patch, as the host assigns it for this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainPatchSlot {
    /// Pool slot, `0..TERRAIN_PATCH_SLOTS`; selects the reserved mesh and the
    /// readback rows.
    pub slot: u32,
    /// Patch column / row inside the cell, `0..TERRAIN_PATCHES_PER_CELL_SIDE`.
    pub patch_col: u32,
    pub patch_row: u32,
    /// Y-up world XZ of the cell's (row 0, col 0) LAND vertex.
    pub cell_origin_xz: [f32; 2],
    /// The cell's terrain `MeshHandle`; its vertex offset is resolved against
    /// the registry at record time, never cached.
    pub base_mesh: u32,
    /// The cell's terrain-tile SSBO slot.
    pub terrain_tile: u32,
    /// Bit `(dr + 1) * 3 + (dc + 1)` set when the neighbouring square at
    /// (`patch_row + dr`, `patch_col + dc`) has no resident patch; squares
    /// across the cell border are always flagged.
    pub missing_mask: u32,
}

/// The displacement settings shared by every slot, world units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainPatchParams {
    pub camera_pos: [f32; 3],
    pub amplitude: f32,
    pub lip: f32,
    pub fade_start: f32,
    pub fade_end: f32,
    pub edge_ramp: f32,
}

/// Push constants — mirrors `terrain_patch.comp`'s `TerrainPatchPush`
/// (std430 push layout: five 16-byte rows).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TerrainPatchPush {
    pub camera_amplitude: [f32; 4],
    pub fade_params: [f32; 4],
    pub cell_origin: [f32; 4],
    pub offsets: [u32; 4],
    pub slot_patch: [u32; 4],
}
// SAFETY: `#[repr(C)]` over `f32`/`u32` arrays only, 80 bytes with no padding.
unsafe impl super::buffer::NoUninit for TerrainPatchPush {}

const PUSH_CONSTANTS_SIZE: u32 = std::mem::size_of::<TerrainPatchPush>() as u32;

impl TerrainPatchPush {
    /// The push block for `slot`, with the two vertex offsets resolved.
    pub fn new(
        params: &TerrainPatchParams,
        slot: &TerrainPatchSlot,
        patch_vertex_offset: u32,
        base_vertex_offset: u32,
    ) -> Self {
        Self {
            camera_amplitude: [
                params.camera_pos[0],
                params.camera_pos[1],
                params.camera_pos[2],
                params.amplitude,
            ],
            fade_params: [params.lip, params.fade_start, params.fade_end, params.edge_ramp],
            cell_origin: [slot.cell_origin_xz[0], slot.cell_origin_xz[1], 0.0, 0.0],
            offsets: [
                patch_vertex_offset,
                base_vertex_offset,
                slot.terrain_tile,
                slot.missing_mask,
            ],
            slot_patch: [slot.slot, slot.patch_col, slot.patch_row, 0],
        }
    }
}

/// What the `terrain.patches` console command reports: the last harvested
/// frame's resident patches and the geometric seam check over their borders.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerrainPatchStats {
    /// Patches dispatched in the harvested frame.
    pub resident: u32,
    /// Borders that must sit at the lip (cell border or missing neighbour),
    /// and the largest |displacement − lip| over their vertices, world units.
    pub lip_edges: u32,
    pub max_lip_deviation: f32,
    /// Borders shared by two resident patches, and the largest world-Y gap
    /// between their coincident vertices.
    pub shared_edges: u32,
    pub max_seam_gap: f32,
    /// The largest displacement anywhere in any resident patch.
    pub max_displacement: f32,
    /// Per slot: (slot, patch col, patch row, cell origin, largest
    /// displacement), in dispatch order.
    pub slots: Vec<(u32, u32, u32, [f32; 2], f32)>,
}

/// The (dr, dc) neighbour of each readback edge, in readback order: row 0,
/// row `QUADS`, column 0, column `QUADS`. Row index grows toward −Z.
const EDGE_NEIGHBOURS: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];

/// The readback edge facing the neighbour that `edge` faces.
const fn opposite_edge(edge: usize) -> usize {
    edge ^ 1
}

/// Mask bit for neighbour (dr, dc).
pub const fn neighbour_bit(dr: i32, dc: i32) -> u32 {
    1 << ((dr + 1) * 3 + (dc + 1))
}

/// (world Y, displacement) of sample `i` of `edge` in one slot's readback words.
fn edge_sample(words: &[u32], edge: usize, i: usize) -> (f32, f32) {
    let w = 2 * (edge * TERRAIN_PATCH_VERTS as usize + i);
    (f32::from_bits(words[w]), f32::from_bits(words[w + 1]))
}

/// The seam check over one harvested frame. `words` holds
/// `TERRAIN_PATCH_READBACK_WORDS` per pool slot, indexed by
/// [`TerrainPatchSlot::slot`].
///
/// A border whose neighbour is flagged missing must hold every vertex at the
/// lip; a border shared with another resident patch of the same cell must
/// agree with that patch's facing border vertex for vertex (both evaluate the
/// same position-only function there, so any gap is a bug).
pub fn summarize_patch_readback(slots: &[TerrainPatchSlot], lip: f32, words: &[u32]) -> TerrainPatchStats {
    let per_slot = TERRAIN_PATCH_READBACK_WORDS as usize;
    let words_of = |slot: u32| -> Option<&[u32]> {
        let start = slot as usize * per_slot;
        words.get(start..start + per_slot)
    };
    let mut stats = TerrainPatchStats {
        resident: slots.len() as u32,
        ..Default::default()
    };
    for slot in slots {
        let Some(own) = words_of(slot.slot) else {
            continue;
        };
        let max_d = f32::from_bits(own[2 * TERRAIN_PATCH_EDGE_SAMPLES as usize]);
        stats.max_displacement = stats.max_displacement.max(max_d);
        stats
            .slots
            .push((slot.slot, slot.patch_col, slot.patch_row, slot.cell_origin_xz, max_d));
        for (edge, &(dr, dc)) in EDGE_NEIGHBOURS.iter().enumerate() {
            if slot.missing_mask & neighbour_bit(dr, dc) != 0 {
                stats.lip_edges += 1;
                for i in 0..TERRAIN_PATCH_VERTS as usize {
                    let (_, d) = edge_sample(own, edge, i);
                    stats.max_lip_deviation = stats.max_lip_deviation.max((d - lip).abs());
                }
                continue;
            }
            // Count each shared border once, from its lower-index side.
            if dr < 0 || dc < 0 {
                continue;
            }
            let neighbour = slots.iter().find(|n| {
                n.base_mesh == slot.base_mesh
                    && n.patch_col as i32 == slot.patch_col as i32 + dc
                    && n.patch_row as i32 == slot.patch_row as i32 + dr
            });
            let Some(other) = neighbour.and_then(|n| words_of(n.slot)) else {
                continue;
            };
            stats.shared_edges += 1;
            for i in 0..TERRAIN_PATCH_VERTS as usize {
                let (y_own, _) = edge_sample(own, edge, i);
                let (y_other, _) = edge_sample(other, opposite_edge(edge), i);
                stats.max_seam_gap = stats.max_seam_gap.max((y_own - y_other).abs());
            }
        }
    }
    stats
}

/// The patch lattice's triangles, the winding `cell_loader/terrain.rs`'s
/// `land_tile_indices` gives the base cell: per quad `tl, tr, bl` then
/// `tr, br, bl`.
pub fn patch_lattice_indices() -> Vec<u32> {
    let verts = TERRAIN_PATCH_VERTS;
    let mut indices = Vec::with_capacity((TERRAIN_PATCH_QUADS * TERRAIN_PATCH_QUADS * 6) as usize);
    for row in 0..TERRAIN_PATCH_QUADS {
        for col in 0..TERRAIN_PATCH_QUADS {
            let tl = row * verts + col;
            let tr = tl + 1;
            let bl = (row + 1) * verts + col;
            let br = bl + 1;
            indices.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
        }
    }
    indices
}

/// Placeholder vertices for a reserved patch mesh: a flat, upward-facing
/// lattice at the origin. The compute pass overwrites every byte before any
/// frame draws the patch, so only the topology and the count matter; the flat
/// lattice keeps the registry's derived bound and texel density finite.
fn placeholder_lattice() -> Vec<Vertex> {
    (0..TERRAIN_PATCH_VERTEX_COUNT)
        .map(|i| {
            let col = (i % TERRAIN_PATCH_VERTS) as f32;
            let row = (i / TERRAIN_PATCH_VERTS) as f32;
            Vertex::new_terrain(
                [col * TERRAIN_PATCH_LATTICE_STEP, 0.0, -row * TERRAIN_PATCH_LATTICE_STEP],
                [1.0, 1.0, 1.0],
                [0.0, 1.0, 0.0],
                [col / TERRAIN_PATCH_QUADS as f32, row / TERRAIN_PATCH_QUADS as f32],
                [0; 4],
                [0; 4],
            )
        })
        .collect()
}

pub(super) fn terrain_patch_layout_bindings() -> Vec<vk::DescriptorSetLayoutBinding<'static>> {
    vec![
        compute_binding(0, vk::DescriptorType::STORAGE_BUFFER),
        compute_binding(1, vk::DescriptorType::STORAGE_BUFFER),
        compute_binding(2, vk::DescriptorType::STORAGE_BUFFER),
    ]
}

/// The buffers a frame's dispatches bind.
#[derive(Clone, Copy, Debug)]
pub struct TerrainPatchBuffers {
    pub vertex_buffer: vk::Buffer,
    pub vertex_buffer_size: vk::DeviceSize,
    /// `MeshRegistry::geometry_generation`, so a recycled buffer handle is
    /// not mistaken for the bound one (#2743).
    pub vertex_generation: u64,
    pub tile_buffer: vk::Buffer,
    pub tile_buffer_size: vk::DeviceSize,
}

pub struct TerrainPatchPass {
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
    /// Per frame slot: the (vertex buffer, generation, tile buffer) its set
    /// was last written with.
    bound: [Option<(vk::Buffer, u64, vk::Buffer)>; MAX_FRAMES_IN_FLIGHT],
    readback: Vec<GpuBuffer>,
    /// Per frame slot: the slots and lip it last dispatched, harvested when
    /// that slot comes round again (its fence has retired by then).
    pending: [Vec<TerrainPatchSlot>; MAX_FRAMES_IN_FLIGHT],
    pending_lip: [f32; MAX_FRAMES_IN_FLIGHT],
    /// The reserved patch meshes, indexed by slot. Empty until
    /// [`Self::reserve_meshes`] succeeds.
    meshes: Vec<u32>,
    /// This frame's input from the host; consumed by [`Self::record`].
    frame_slots: Vec<TerrainPatchSlot>,
    frame_params: Option<TerrainPatchParams>,
    stats: TerrainPatchStats,
}

impl TerrainPatchPass {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        bindless_layout: vk::DescriptorSetLayout,
    ) -> Result<Self> {
        let bindings = terrain_patch_layout_bindings();
        validate_set_layout(
            0,
            &bindings,
            &[ReflectedShader {
                name: "terrain_patch.comp",
                spirv: TERRAIN_PATCH_COMP_SPV,
            }],
            "terrain_patch",
            &[],
        )
        .expect("terrain_patch layout drifted against terrain_patch.comp");

        // SAFETY: `device` is live; the create-info borrows only `bindings`.
        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .context("create terrain_patch descriptor set layout")?
        };
        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .offset(0)
            .size(PUSH_CONSTANTS_SIZE);
        let set_layouts = [descriptor_set_layout, bindless_layout];
        // SAFETY: both set layouts are live for this call.
        let pipeline_layout = match unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&set_layouts)
                    .push_constant_ranges(std::slice::from_ref(&push_range)),
                None,
            )
        } {
            Ok(layout) => layout,
            Err(e) => {
                // SAFETY: just created above, referenced by nothing.
                unsafe { device.destroy_descriptor_set_layout(descriptor_set_layout, None) };
                return Err(e).context("create terrain_patch pipeline layout");
            }
        };
        let mut this = Self {
            pipeline: vk::Pipeline::null(),
            pipeline_layout,
            descriptor_set_layout,
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: Vec::new(),
            bound: [None; MAX_FRAMES_IN_FLIGHT],
            readback: Vec::new(),
            pending: std::array::from_fn(|_| Vec::new()),
            pending_lip: [0.0; MAX_FRAMES_IN_FLIGHT],
            meshes: Vec::new(),
            frame_slots: Vec::new(),
            frame_params: None,
            stats: TerrainPatchStats::default(),
        };
        if let Err(e) = this.create_resources(device, allocator, pipeline_cache, &bindings) {
            this.destroy(device, allocator);
            return Err(e);
        }
        log::info!(
            "Terrain patch pass created ({} slots × {} vertices, push={} B)",
            TERRAIN_PATCH_SLOTS,
            TERRAIN_PATCH_VERTEX_COUNT,
            PUSH_CONSTANTS_SIZE,
        );
        Ok(this)
    }

    fn create_resources(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        bindings: &[vk::DescriptorSetLayoutBinding<'_>],
    ) -> Result<()> {
        self.pipeline = super::pipeline::create_compute_pipeline(
            device,
            pipeline_cache,
            TERRAIN_PATCH_COMP_SPV,
            self.pipeline_layout,
            "terrain_patch",
        )?;
        let sets = MAX_FRAMES_IN_FLIGHT as u32;
        self.descriptor_pool = DescriptorPoolBuilder::from_layout_bindings(bindings, sets)
            .max_sets(sets)
            .build(device, "create terrain_patch descriptor pool")?;
        let layouts = [self.descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        // SAFETY: the pool was sized for exactly these sets.
        self.descriptor_sets = unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(self.descriptor_pool)
                        .set_layouts(&layouts),
                )
                .context("allocate terrain_patch descriptor sets")?
        };
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            self.readback.push(GpuBuffer::create_host_readback(
                device,
                allocator,
                READBACK_BYTES,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            )?);
        }
        Ok(())
    }

    /// Reserve the [`TERRAIN_PATCH_SLOTS`] patch meshes in the global pool,
    /// once. Idempotent; a failure leaves the pool unreserved (patches stay
    /// off) and is retried on the next call.
    pub fn reserve_meshes(&mut self, registry: &mut MeshRegistry) -> Result<()> {
        if !self.meshes.is_empty() {
            return Ok(());
        }
        let vertices = placeholder_lattice();
        let indices = patch_lattice_indices();
        let mut meshes = Vec::with_capacity(TERRAIN_PATCH_SLOTS as usize);
        for _ in 0..TERRAIN_PATCH_SLOTS {
            match registry.upload_scene_mesh_global_only(&vertices, &indices) {
                Ok(handle) => meshes.push(handle),
                Err(e) => {
                    for handle in meshes {
                        registry.drop_mesh(handle);
                    }
                    return Err(e).context("reserve terrain patch meshes");
                }
            }
        }
        self.meshes = meshes;
        Ok(())
    }

    /// The reserved patch mesh handles, indexed by slot (empty before
    /// [`Self::reserve_meshes`]).
    pub fn meshes(&self) -> &[u32] {
        &self.meshes
    }

    /// Stage this frame's resident patches. Slots outside the pool or with a
    /// duplicate index are dropped.
    pub fn set_frame(&mut self, slots: &[TerrainPatchSlot], params: TerrainPatchParams) {
        self.frame_slots.clear();
        for slot in slots {
            if slot.slot < TERRAIN_PATCH_SLOTS
                && !self.frame_slots.iter().any(|s| s.slot == slot.slot)
            {
                self.frame_slots.push(*slot);
            }
        }
        self.frame_params = Some(params);
    }

    /// Whether [`Self::record`] has anything to dispatch this frame.
    pub fn has_frame(&self) -> bool {
        self.frame_params.is_some() && !self.meshes.is_empty() && !self.frame_slots.is_empty()
    }

    /// The last harvested frame's report.
    pub fn stats(&self) -> &TerrainPatchStats {
        &self.stats
    }

    /// Read back what `frame` dispatched last time round. Its fence has
    /// retired: `draw_frame` waits it before recording into the slot.
    fn harvest(&mut self, device: &ash::Device, frame: usize) {
        let slots = std::mem::take(&mut self.pending[frame]);
        if slots.is_empty() {
            self.stats = TerrainPatchStats::default();
            return;
        }
        let lip = self.pending_lip[frame];
        let buffer = &mut self.readback[frame];
        if buffer.invalidate_if_needed(device).is_err() {
            return;
        }
        let Ok(bytes) = buffer.mapped_slice_mut() else {
            return;
        };
        let words: Vec<u32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| u32::from_ne_bytes(*w))
            .collect();
        self.stats = summarize_patch_readback(&slots, lip, &words);
    }

    /// Record this frame's dispatches into `cmd`: zero the readback, then one
    /// dispatch per resident slot, bracketed by the barriers that order them
    /// after earlier geometry writes / reads and before this frame's vertex
    /// fetch and the host readback. Returns the number of slots dispatched.
    ///
    /// # Safety
    /// `cmd` must be recording outside a render pass; `buffers` must be the
    /// global vertex buffer and terrain-tile SSBO this frame's draws read, and
    /// `bindless_set` the texture registry's set for `frame`.
    pub unsafe fn record(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        buffers: TerrainPatchBuffers,
        bindless_set: vk::DescriptorSet,
        registry: &MeshRegistry,
    ) -> u32 {
        self.harvest(device, frame);
        let Some(params) = self.frame_params.take() else {
            return 0;
        };
        if self.meshes.is_empty() || self.frame_slots.is_empty() {
            self.frame_slots.clear();
            return 0;
        }
        // Resolve both offsets now: compaction can move either mesh between
        // frames, so a cached offset would write someone else's vertices.
        let mut pushes = Vec::with_capacity(self.frame_slots.len());
        let mut dispatched = Vec::with_capacity(self.frame_slots.len());
        for slot in &self.frame_slots {
            let (Some(patch), Some(base)) = (
                self.meshes.get(slot.slot as usize).and_then(|&h| registry.get(h)),
                registry.get(slot.base_mesh),
            ) else {
                continue;
            };
            if base.vertex_count != crate::shader_constants::LAND_GRID_VERTS.pow(2)
                || patch.vertex_count != TERRAIN_PATCH_VERTEX_COUNT
            {
                continue;
            }
            pushes.push(TerrainPatchPush::new(
                &params,
                slot,
                patch.global_vertex_offset,
                base.global_vertex_offset,
            ));
            dispatched.push(*slot);
        }
        self.frame_slots.clear();
        if pushes.is_empty() {
            return 0;
        }

        // SAFETY: caller contract — `cmd` is recording outside a render pass,
        // and every bound buffer is live for this frame's submission.
        unsafe {
            let set = self.descriptor_sets[frame];
            let key = (buffers.vertex_buffer, buffers.vertex_generation, buffers.tile_buffer);
            if self.bound[frame] != Some(key) {
                let vertex_info = [vk::DescriptorBufferInfo {
                    buffer: buffers.vertex_buffer,
                    offset: 0,
                    range: buffers.vertex_buffer_size,
                }];
                let tile_info = [vk::DescriptorBufferInfo {
                    buffer: buffers.tile_buffer,
                    offset: 0,
                    range: buffers.tile_buffer_size,
                }];
                let readback_info = [vk::DescriptorBufferInfo {
                    buffer: self.readback[frame].buffer,
                    offset: 0,
                    range: READBACK_BYTES,
                }];
                device.update_descriptor_sets(
                    &[
                        write_storage_buffer(set, 0, &vertex_info),
                        write_storage_buffer(set, 1, &tile_info),
                        write_storage_buffer(set, 2, &readback_info),
                    ],
                    &[],
                );
                self.bound[frame] = Some(key);
            }

            device.cmd_fill_buffer(cmd, self.readback[frame].buffer, 0, READBACK_BYTES, 0);
            // Before: the readback zero-fill and any geometry upload / tile
            // copy (TRANSFER writes) must land, and every earlier vertex fetch
            // or shader read of the patch ranges (the previous frame's draws)
            // must finish before the overwrite.
            let before = vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER
                    | vk::PipelineStageFlags::VERTEX_INPUT
                    | vk::PipelineStageFlags::VERTEX_SHADER
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[before],
                &[],
                &[],
            );

            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline_layout,
                0,
                &[set, bindless_set],
                &[],
            );
            let groups = TERRAIN_PATCH_VERTEX_COUNT.div_ceil(TERRAIN_PATCH_WORKGROUP);
            for push in &pushes {
                device.cmd_push_constants(
                    cmd,
                    self.pipeline_layout,
                    vk::ShaderStageFlags::COMPUTE,
                    0,
                    super::buffer::byte_view(std::slice::from_ref(push)),
                );
                device.cmd_dispatch(cmd, groups, 1, 1);
            }

            // After: this frame's vertex fetch and shader reads see the
            // patches, and the host can read the readback once the fence
            // retires.
            let after = vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(
                    vk::AccessFlags::VERTEX_ATTRIBUTE_READ
                        | vk::AccessFlags::SHADER_READ
                        | vk::AccessFlags::HOST_READ,
                );
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::VERTEX_INPUT
                    | vk::PipelineStageFlags::VERTEX_SHADER
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COMPUTE_SHADER
                    | vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[after],
                &[],
                &[],
            );
        }
        self.pending_lip[frame] = params.lip;
        let count = dispatched.len() as u32;
        self.pending[frame] = dispatched;
        count
    }

    /// Tear down. The reserved meshes belong to the registry and go with it.
    pub fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for buffer in &mut self.readback {
            buffer.destroy(device, allocator);
        }
        self.readback.clear();
        // SAFETY: called after the device is idle (teardown); every handle was
        // created by this pass and is destroyed once.
        unsafe {
            if self.descriptor_pool != vk::DescriptorPool::null() {
                device.destroy_descriptor_pool(self.descriptor_pool, None);
                self.descriptor_pool = vk::DescriptorPool::null();
            }
            if self.pipeline != vk::Pipeline::null() {
                device.destroy_pipeline(self.pipeline, None);
                self.pipeline = vk::Pipeline::null();
            }
            if self.pipeline_layout != vk::PipelineLayout::null() {
                device.destroy_pipeline_layout(self.pipeline_layout, None);
                self.pipeline_layout = vk::PipelineLayout::null();
            }
            if self.descriptor_set_layout != vk::DescriptorSetLayout::null() {
                device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
                self.descriptor_set_layout = vk::DescriptorSetLayout::null();
            }
        }
        self.descriptor_sets.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_constants_are_80_bytes() {
        assert_eq!(PUSH_CONSTANTS_SIZE, 80);
    }

    #[test]
    fn the_lattice_matches_the_design_numbers() {
        // §3.1: 3.66 m squares of 64×64 quads at 5.7 cm, 16×16 per cell.
        assert_eq!(TERRAIN_PATCH_VERTEX_COUNT, 4225);
        assert_eq!(TERRAIN_PATCH_LATTICE_STEP, 4.0);
        assert_eq!(
            TERRAIN_PATCH_LATTICE_STEP * TERRAIN_PATCH_QUADS as f32
                * crate::shader_constants::TERRAIN_PATCHES_PER_CELL_SIDE as f32,
            crate::shader_constants::EXTERIOR_CELL_UNITS
        );
        assert_eq!(patch_lattice_indices().len(), 64 * 64 * 6);
    }

    #[test]
    fn the_lattice_faces_up_like_the_base_cell() {
        let vertices = placeholder_lattice();
        for tri in patch_lattice_indices().as_chunks::<3>().0 {
            let [a, b, c] = tri.map(|i| vertices[i as usize].position);
            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let ny = ab[2] * ac[0] - ab[0] * ac[2];
            assert!(ny > 0.0, "triangle {tri:?} faces down");
        }
    }

    fn slot(slot: u32, col: u32, row: u32, missing_mask: u32) -> TerrainPatchSlot {
        TerrainPatchSlot {
            slot,
            patch_col: col,
            patch_row: row,
            cell_origin_xz: [0.0, 0.0],
            base_mesh: 7,
            terrain_tile: 0,
            missing_mask,
        }
    }

    /// A readback where every sample sits at `y`/`d` and the slot max is `max_d`.
    fn fill(words: &mut [u32], slot: u32, y: f32, d: f32, max_d: f32) {
        let base = slot as usize * TERRAIN_PATCH_READBACK_WORDS as usize;
        for i in 0..TERRAIN_PATCH_EDGE_SAMPLES as usize {
            words[base + 2 * i] = y.to_bits();
            words[base + 2 * i + 1] = d.to_bits();
        }
        words[base + 2 * TERRAIN_PATCH_EDGE_SAMPLES as usize] = max_d.to_bits();
    }

    const ALL_BUT_SELF: u32 = 0b111_101_111;

    #[test]
    fn two_neighbours_share_one_edge_and_flag_the_rest_as_lip() {
        let lip = 0.07;
        // Slot 0 at (0,0), slot 1 east of it at col 1.
        let a = slot(0, 0, 0, ALL_BUT_SELF & !neighbour_bit(0, 1));
        let b = slot(1, 1, 0, ALL_BUT_SELF & !neighbour_bit(0, -1));
        let mut words = vec![0u32; READBACK_BYTES as usize / 4];
        fill(&mut words, 0, 10.0, lip, 1.5);
        fill(&mut words, 1, 10.0, lip, 2.0);
        let stats = summarize_patch_readback(&[a, b], lip, &words);
        assert_eq!(stats.resident, 2);
        assert_eq!(stats.shared_edges, 1, "the a/b border counts once");
        assert_eq!(stats.lip_edges, 6);
        assert_eq!(stats.max_seam_gap, 0.0);
        assert_eq!(stats.max_lip_deviation, 0.0);
        assert_eq!(stats.max_displacement, 2.0);
    }

    #[test]
    fn a_gap_on_a_shared_edge_and_a_raised_lip_are_reported() {
        let lip = 0.07;
        let a = slot(0, 0, 0, ALL_BUT_SELF & !neighbour_bit(0, 1));
        let b = slot(1, 1, 0, ALL_BUT_SELF & !neighbour_bit(0, -1));
        let mut words = vec![0u32; READBACK_BYTES as usize / 4];
        fill(&mut words, 0, 10.0, lip, 1.0);
        fill(&mut words, 1, 10.0, lip, 1.0);
        // b's west edge (column 0, edge 2), sample 5, sits 0.25 high.
        let base = TERRAIN_PATCH_READBACK_WORDS as usize;
        let w = base + 2 * (2 * TERRAIN_PATCH_VERTS as usize + 5);
        words[w] = 10.25f32.to_bits();
        // a's north edge (row 0, edge 0), sample 3, is displaced past the lip.
        words[2 * 3 + 1] = 0.5f32.to_bits();
        let stats = summarize_patch_readback(&[a, b], lip, &words);
        assert!((stats.max_seam_gap - 0.25).abs() < 1e-6);
        assert!((stats.max_lip_deviation - 0.43).abs() < 1e-6);
    }

    #[test]
    fn neighbours_in_another_cell_are_not_compared() {
        let lip = 0.07;
        let a = slot(0, 0, 0, ALL_BUT_SELF & !neighbour_bit(0, 1));
        let mut b = slot(1, 1, 0, ALL_BUT_SELF);
        b.base_mesh = 8;
        let mut words = vec![0u32; READBACK_BYTES as usize / 4];
        fill(&mut words, 0, 10.0, lip, 1.0);
        fill(&mut words, 1, 99.0, lip, 1.0);
        let stats = summarize_patch_readback(&[a, b], lip, &words);
        assert_eq!(stats.shared_edges, 0);
    }
}
