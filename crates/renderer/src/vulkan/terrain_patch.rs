//! TSD-4.1 / TSD-4.2 terrain displacement patches (#7152, #7221).
//!
//! Design: `docs/engine/terrain-displacement.md` §3. A fixed pool of
//! [`TERRAIN_PATCH_SLOTS`] slots is reserved once in the global vertex pool
//! ([`TerrainPatchPass::reserve_meshes`], global-pool-only uploads: no per-mesh
//! buffers; the patch BLAS set builds their acceleration structures, not the
//! reservation), one mesh of each lattice level per slot: the fine 64×64
//! lattice and the coarse 32×32 one over its even points, which the host picks
//! by distance ([`TerrainPatchSlot::coarse`]). Every frame the host hands over the resident
//! slots ([`TerrainPatchSlot`]) and the shared parameters
//! ([`TerrainPatchParams`]); [`TerrainPatchPass::record`] re-displaces each
//! resident slot in place with `terrain_patch.comp`, after the terrain-tile
//! upload it reads and before the patch BLAS refit and the TLAS build
//! (`draw.rs`, ahead of `dispatch_skin_and_cluster`). The raster path draws the
//! patches from the same bytes (the host clones the base cell's draw command
//! onto the patch mesh, with `in_tlas` set from the base's and the
//! `terrain.patches rt` switch); a patch draw enters the TLAS once its BLAS
//! matches this frame (TSD-4.2).
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
    TERRAIN_PATCH_COARSE_MASK_SHIFT, TERRAIN_PATCH_COARSE_STEP, TERRAIN_PATCH_EDGE_SAMPLES,
    TERRAIN_PATCH_LATTICE_STEP, TERRAIN_PATCH_QUADS, TERRAIN_PATCH_READBACK_WORDS,
    TERRAIN_PATCH_SLOTS, TERRAIN_PATCH_VERTS, TERRAIN_PATCH_WORKGROUP,
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
    /// This slot draws its coarse mesh (every second lattice point).
    pub coarse: bool,
    /// Same bits as `missing_mask`, set where the neighbour is resident and
    /// coarse: a fine slot stitches its border to it.
    pub coarse_mask: u32,
}

impl TerrainPatchSlot {
    /// Lattice steps between this slot's vertices.
    pub const fn step(&self) -> u32 {
        if self.coarse {
            TERRAIN_PATCH_COARSE_STEP
        } else {
            1
        }
    }

    /// Vertices along one side of this slot's mesh.
    pub const fn verts_per_side(&self) -> u32 {
        TERRAIN_PATCH_QUADS / self.step() + 1
    }

    pub const fn vertex_count(&self) -> u32 {
        self.verts_per_side() * self.verts_per_side()
    }
}

/// The reserved patch meshes, indexed by slot: one per lattice level.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerrainPatchMeshes {
    pub fine: Vec<u32>,
    pub coarse: Vec<u32>,
}

impl TerrainPatchMeshes {
    pub fn is_empty(&self) -> bool {
        self.fine.is_empty() || self.coarse.is_empty()
    }

    /// Whether `mesh` is one of the reserved patch meshes.
    pub fn contains(&self, mesh: u32) -> bool {
        self.fine.contains(&mesh) || self.coarse.contains(&mesh)
    }

    /// The mesh `slot` draws at its level.
    pub fn get(&self, slot: u32, coarse: bool) -> Option<u32> {
        let level = if coarse { &self.coarse } else { &self.fine };
        level.get(slot as usize).copied()
    }
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
    /// Keep the TSD-3 parallax march on patch draws. On by default: the 3 cm
    /// patch relief alone reads flatter than the 8 cm parallax depth, so
    /// dropping the march flattens the ground nearest the camera; marching
    /// only the residual relief is stage 4.3 (design §3.6). `false` is the
    /// A/B switch.
    pub parallax: bool,
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
                slot.missing_mask | (slot.coarse_mask << TERRAIN_PATCH_COARSE_MASK_SHIFT),
            ],
            slot_patch: [slot.slot, slot.patch_col, slot.patch_row, slot.step()],
        }
    }
}

/// What the `terrain.patches` console command reports: the last harvested
/// frame's resident patches and the geometric seam check over their borders.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerrainPatchStats {
    /// Patches dispatched in the harvested frame, and how many of them at the
    /// coarse level.
    pub resident: u32,
    pub coarse: u32,
    /// Borders that must sit at the lip (cell border or missing neighbour),
    /// and the largest |displacement − lip| over their vertices, world units.
    pub lip_edges: u32,
    pub max_lip_deviation: f32,
    /// Borders shared by two resident patches (`level_edges` of them between
    /// a fine and a coarse patch), and the largest world-Y gap between their
    /// coincident vertices — on a level border, between the fine patch's odd
    /// vertex and the coarse edge's midpoint under it.
    pub shared_edges: u32,
    pub level_edges: u32,
    pub max_seam_gap: f32,
    /// The largest displacement anywhere in any resident patch.
    pub max_displacement: f32,
    /// Per slot, in dispatch order.
    pub slots: Vec<TerrainPatchSlotStats>,
    /// #7221 — this frame's patch BLAS builds / refits.
    pub blas: crate::vulkan::acceleration::TerrainPatchBlasStats,
}

/// One harvested slot, for the `terrain.patches` listing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TerrainPatchSlotStats {
    pub slot: u32,
    pub patch_col: u32,
    pub patch_row: u32,
    pub cell_origin_xz: [f32; 2],
    pub coarse: bool,
    /// Largest displacement in the slot, world units.
    pub max_displacement: f32,
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

/// (world Y, displacement) of sample `i` of `edge` in one slot's readback
/// words; `None` where the slot wrote nothing (a coarse slot's odd samples —
/// every written displacement is at least the lip, so never zero bits).
fn edge_sample(words: &[u32], edge: usize, i: usize) -> Option<(f32, f32)> {
    let w = 2 * (edge * TERRAIN_PATCH_VERTS as usize + i);
    (words[w + 1] != 0).then(|| (f32::from_bits(words[w]), f32::from_bits(words[w + 1])))
}

/// The seam check over one harvested frame. `words` holds
/// `TERRAIN_PATCH_READBACK_WORDS` per pool slot, indexed by
/// [`TerrainPatchSlot::slot`].
///
/// A border whose neighbour is flagged missing must hold every vertex at the
/// lip; a border shared with another resident patch of the same cell must
/// agree with that patch's facing border vertex for vertex (both evaluate the
/// same position-only function there, so any gap is a bug). Where only one side
/// has a vertex (a fine patch's odd vertex beside a coarse patch), it must sit
/// at the midpoint of the other side's two neighbouring vertices — on the
/// coarse edge the rasteriser draws.
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
    let verts = TERRAIN_PATCH_VERTS as usize;
    for slot in slots {
        let Some(own) = words_of(slot.slot) else {
            continue;
        };
        let max_d = f32::from_bits(own[2 * TERRAIN_PATCH_EDGE_SAMPLES as usize]);
        stats.max_displacement = stats.max_displacement.max(max_d);
        stats.coarse += u32::from(slot.coarse);
        stats.slots.push(TerrainPatchSlotStats {
            slot: slot.slot,
            patch_col: slot.patch_col,
            patch_row: slot.patch_row,
            cell_origin_xz: slot.cell_origin_xz,
            coarse: slot.coarse,
            max_displacement: max_d,
        });
        for (edge, &(dr, dc)) in EDGE_NEIGHBOURS.iter().enumerate() {
            if slot.missing_mask & neighbour_bit(dr, dc) != 0 {
                stats.lip_edges += 1;
                for i in 0..verts {
                    if let Some((_, d)) = edge_sample(own, edge, i) {
                        stats.max_lip_deviation = stats.max_lip_deviation.max((d - lip).abs());
                    }
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
            let Some((other_slot, other)) =
                neighbour.and_then(|n| words_of(n.slot).map(|w| (n, w)))
            else {
                continue;
            };
            stats.shared_edges += 1;
            stats.level_edges += u32::from(other_slot.coarse != slot.coarse);
            let facing = opposite_edge(edge);
            let y = |words: &[u32], e: usize, i: usize| edge_sample(words, e, i).map(|s| s.0);
            // The midpoint of `words`' samples either side of `i`.
            let between = |words: &[u32], e: usize, i: usize| -> Option<f32> {
                (i > 0 && i + 1 < verts)
                    .then(|| Some(0.5 * (y(words, e, i - 1)? + y(words, e, i + 1)?)))
                    .flatten()
            };
            for i in 0..verts {
                let gap = match (y(own, edge, i), y(other, facing, i)) {
                    (Some(a), Some(b)) => Some(a - b),
                    (Some(a), None) => between(other, facing, i).map(|b| a - b),
                    (None, Some(b)) => between(own, edge, i).map(|a| a - b),
                    (None, None) => None,
                };
                if let Some(gap) = gap {
                    stats.max_seam_gap = stats.max_seam_gap.max(gap.abs());
                }
            }
        }
    }
    stats
}

/// Which square a slot shows, for the patch BLAS's refit-or-build decision: a
/// mesh refits only while it keeps showing the same square of the same cell.
pub fn patch_square_key(slot: &TerrainPatchSlot) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    slot.cell_origin_xz.map(f32::to_bits).hash(&mut hasher);
    (slot.patch_col, slot.patch_row, slot.base_mesh, slot.coarse).hash(&mut hasher);
    hasher.finish()
}

/// A `quads`×`quads` patch lattice's triangles, the winding
/// `cell_loader/terrain.rs`'s `land_tile_indices` gives the base cell: per quad
/// `tl, tr, bl` then `tr, br, bl`.
pub fn patch_lattice_indices(quads: u32) -> Vec<u32> {
    let verts = quads + 1;
    let mut indices = Vec::with_capacity((quads * quads * 6) as usize);
    for row in 0..quads {
        for col in 0..quads {
            let tl = row * verts + col;
            let tr = tl + 1;
            let bl = (row + 1) * verts + col;
            let br = bl + 1;
            indices.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
        }
    }
    indices
}

/// Placeholder vertices for a reserved patch mesh with `step` lattice steps
/// between vertices: a flat, upward-facing lattice at the origin. The compute
/// pass overwrites every byte before any frame draws the patch, so only the
/// topology and the count matter; the flat lattice keeps the registry's derived
/// bound and texel density finite.
fn placeholder_lattice(step: u32) -> Vec<Vertex> {
    let quads = TERRAIN_PATCH_QUADS / step;
    let verts = quads + 1;
    let spacing = TERRAIN_PATCH_LATTICE_STEP * step as f32;
    (0..verts * verts)
        .map(|i| {
            let col = (i % verts) as f32;
            let row = (i / verts) as f32;
            Vertex::new_terrain(
                [col * spacing, 0.0, -row * spacing],
                [1.0, 1.0, 1.0],
                [0.0, 1.0, 0.0],
                [col / quads as f32, row / quads as f32],
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
    meshes: TerrainPatchMeshes,
    /// This frame's input from the host; consumed by [`Self::record`].
    frame_slots: Vec<TerrainPatchSlot>,
    frame_params: Option<TerrainPatchParams>,
    /// The last staged `TerrainPatchParams::parallax`.
    parallax: bool,
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
            meshes: TerrainPatchMeshes::default(),
            frame_slots: Vec::new(),
            frame_params: None,
            parallax: true,
            stats: TerrainPatchStats::default(),
        };
        if let Err(e) = this.create_resources(device, allocator, pipeline_cache, &bindings) {
            this.destroy(device, allocator);
            return Err(e);
        }
        log::info!(
            "Terrain patch pass created ({} slots × {}/{} vertices fine/coarse, push={} B)",
            TERRAIN_PATCH_SLOTS,
            crate::shader_constants::TERRAIN_PATCH_VERTEX_COUNT,
            crate::shader_constants::TERRAIN_PATCH_COARSE_VERTEX_COUNT,
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

    /// Reserve the [`TERRAIN_PATCH_SLOTS`] patch meshes of each level in the
    /// global pool, once. Idempotent; a failure leaves the pool unreserved
    /// (patches stay off) and is retried on the next call.
    pub fn reserve_meshes(&mut self, registry: &mut MeshRegistry) -> Result<()> {
        if !self.meshes.is_empty() {
            return Ok(());
        }
        let mut reserved: Vec<u32> = Vec::with_capacity(2 * TERRAIN_PATCH_SLOTS as usize);
        for step in [1, TERRAIN_PATCH_COARSE_STEP] {
            let vertices = placeholder_lattice(step);
            let indices = patch_lattice_indices(TERRAIN_PATCH_QUADS / step);
            for _ in 0..TERRAIN_PATCH_SLOTS {
                match registry.upload_scene_mesh_global_only(&vertices, &indices) {
                    Ok(handle) => reserved.push(handle),
                    Err(e) => {
                        for handle in reserved {
                            registry.drop_mesh(handle);
                        }
                        return Err(e).context("reserve terrain patch meshes");
                    }
                }
            }
        }
        let coarse = reserved.split_off(TERRAIN_PATCH_SLOTS as usize);
        self.meshes = TerrainPatchMeshes { fine: reserved, coarse };
        Ok(())
    }

    /// The reserved patch mesh handles (empty before [`Self::reserve_meshes`]).
    pub fn meshes(&self) -> &TerrainPatchMeshes {
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
        self.parallax = params.parallax;
        self.frame_params = Some(params);
    }

    /// Whether a draw of `mesh` skips the TSD-3 parallax march: a patch mesh,
    /// when the host turned parallax off on patches.
    pub fn skips_parallax(&self, mesh: u32) -> bool {
        !self.parallax && self.meshes.contains(mesh)
    }

    /// Whether [`Self::record`] has anything to dispatch this frame.
    pub fn has_frame(&self) -> bool {
        self.frame_params.is_some() && !self.meshes.is_empty() && !self.frame_slots.is_empty()
    }

    /// The last harvested frame's report.
    pub fn stats(&self) -> &TerrainPatchStats {
        &self.stats
    }

    /// Record this frame's patch BLAS counts (after [`Self::record`]'s
    /// harvest replaced the rest of the report).
    pub fn set_blas_stats(&mut self, blas: crate::vulkan::acceleration::TerrainPatchBlasStats) {
        self.stats.blas = blas;
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
                self.meshes.get(slot.slot, slot.coarse).and_then(|h| registry.get(h)),
                registry.get(slot.base_mesh),
            ) else {
                continue;
            };
            if base.vertex_count != crate::shader_constants::LAND_GRID_VERTS.pow(2)
                || patch.vertex_count != slot.vertex_count()
            {
                continue;
            }
            pushes.push((
                TerrainPatchPush::new(
                    &params,
                    slot,
                    patch.global_vertex_offset,
                    base.global_vertex_offset,
                ),
                slot.vertex_count().div_ceil(TERRAIN_PATCH_WORKGROUP),
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
            for (push, groups) in &pushes {
                device.cmd_push_constants(
                    cmd,
                    self.pipeline_layout,
                    vk::ShaderStageFlags::COMPUTE,
                    0,
                    super::buffer::byte_view(std::slice::from_ref(push)),
                );
                device.cmd_dispatch(cmd, *groups, 1, 1);
            }

            // After: this frame's vertex fetch, shader reads and patch BLAS
            // builds (#7221: AS-build geometry reads are SHADER_READ in the
            // AS_BUILD stage) see the patches, and the host can read the
            // readback once the fence retires.
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
                    | vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR
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

    /// The patch BLAS work for what [`Self::record`] just dispatched into
    /// `frame`, appended to `out` (the acceleration manager's kept list,
    /// #7246): one request per dispatched slot, at the mesh of its level.
    /// `vertex_base` / `index_base` are the global vertex / index buffers'
    /// device addresses.
    pub fn blas_requests(
        &self,
        frame: usize,
        registry: &MeshRegistry,
        vertex_base: vk::DeviceAddress,
        index_base: vk::DeviceAddress,
        out: &mut Vec<crate::vulkan::acceleration::TerrainPatchBlasRequest>,
    ) {
        let vertex_stride = std::mem::size_of::<Vertex>() as vk::DeviceAddress;
        out.extend(self.pending[frame]
            .iter()
            .filter_map(|slot| {
                let mesh = self.meshes.get(slot.slot, slot.coarse)?;
                let entry = registry.get(mesh)?;
                Some(crate::vulkan::acceleration::TerrainPatchBlasRequest {
                    mesh,
                    content: crate::vulkan::acceleration::TerrainPatchBlasContent {
                        square: patch_square_key(slot),
                        vertex_address: vertex_base
                            + vk::DeviceAddress::from(entry.global_vertex_offset) * vertex_stride,
                        index_address: index_base
                            + vk::DeviceAddress::from(entry.global_index_offset) * 4,
                    },
                    vertex_count: entry.vertex_count,
                    index_count: entry.index_count,
                })
            }));
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
        assert_eq!(crate::shader_constants::TERRAIN_PATCH_VERTEX_COUNT, 4225);
        assert_eq!(crate::shader_constants::TERRAIN_PATCH_COARSE_VERTEX_COUNT, 33 * 33);
        assert_eq!(TERRAIN_PATCH_LATTICE_STEP, 4.0);
        assert_eq!(
            TERRAIN_PATCH_LATTICE_STEP * TERRAIN_PATCH_QUADS as f32
                * crate::shader_constants::TERRAIN_PATCHES_PER_CELL_SIDE as f32,
            crate::shader_constants::EXTERIOR_CELL_UNITS
        );
        assert_eq!(patch_lattice_indices(64).len(), 64 * 64 * 6);
    }

    #[test]
    fn both_lattices_face_up_like_the_base_cell_and_span_one_square() {
        for step in [1, TERRAIN_PATCH_COARSE_STEP] {
            let vertices = placeholder_lattice(step);
            let quads = TERRAIN_PATCH_QUADS / step;
            assert_eq!(vertices.len() as u32, (quads + 1) * (quads + 1));
            let far = vertices.last().unwrap().position;
            let side = TERRAIN_PATCH_LATTICE_STEP * TERRAIN_PATCH_QUADS as f32;
            assert_eq!([far[0], far[2]], [side, -side], "step {step}");
            for tri in patch_lattice_indices(quads).as_chunks::<3>().0 {
                let [a, b, c] = tri.map(|i| vertices[i as usize].position);
                let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                let ny = ab[2] * ac[0] - ab[0] * ac[2];
                assert!(ny > 0.0, "step {step} triangle {tri:?} faces down");
            }
        }
    }

    #[test]
    fn the_patch_mesh_lookup_covers_both_levels_and_nothing_else() {
        let meshes = TerrainPatchMeshes { fine: vec![1, 2], coarse: vec![3, 4] };
        for mesh in 1..=4 {
            assert!(meshes.contains(mesh), "mesh {mesh}");
        }
        assert!(!meshes.contains(0) && !meshes.contains(5));
        assert_eq!(meshes.get(1, false), Some(2));
        assert_eq!(meshes.get(1, true), Some(4));
        assert!(TerrainPatchMeshes::default().is_empty());
    }

    #[test]
    fn the_push_carries_the_level_and_both_masks() {
        let params = TerrainPatchParams {
            camera_pos: [0.0; 3],
            amplitude: 1.0,
            lip: 0.07,
            fade_start: 1.0,
            fade_end: 2.0,
            edge_ramp: 70.0,
            parallax: false,
        };
        let mut s = slot(3, 4, 5, neighbour_bit(-1, 0));
        s.coarse_mask = neighbour_bit(0, 1);
        let fine = TerrainPatchPush::new(&params, &s, 100, 200);
        assert_eq!(fine.slot_patch, [3, 4, 5, 1]);
        assert_eq!(
            fine.offsets[3],
            neighbour_bit(-1, 0) | neighbour_bit(0, 1) << TERRAIN_PATCH_COARSE_MASK_SHIFT
        );
        assert!(neighbour_bit(1, 1) < 1 << TERRAIN_PATCH_COARSE_MASK_SHIFT);
        s.coarse = true;
        assert_eq!(TerrainPatchPush::new(&params, &s, 100, 200).slot_patch[3], 2);
        assert_eq!(s.vertex_count(), 33 * 33);
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
            coarse: false,
            coarse_mask: 0,
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

    /// Write sample `i` of `edge` for `slot`.
    fn put(words: &mut [u32], slot: u32, edge: usize, i: usize, y: f32, d: f32) {
        let w = slot as usize * TERRAIN_PATCH_READBACK_WORDS as usize
            + 2 * (edge * TERRAIN_PATCH_VERTS as usize + i);
        words[w] = y.to_bits();
        words[w + 1] = d.to_bits();
    }

    #[test]
    fn a_fine_coarse_border_checks_the_stitch_against_the_coarse_edge() {
        let lip = 0.07;
        // Fine a at col 0, coarse b east of it at col 1.
        let mut a = slot(0, 0, 0, ALL_BUT_SELF & !neighbour_bit(0, 1));
        a.coarse_mask = neighbour_bit(0, 1);
        let mut b = slot(1, 1, 0, ALL_BUT_SELF & !neighbour_bit(0, -1));
        b.coarse = true;
        let mut words = vec![0u32; READBACK_BYTES as usize / 4];
        fill(&mut words, 0, 10.0, lip, 1.0);
        fill(&mut words, 1, 10.0, lip, 1.0);
        // b is coarse: blank its odd samples; its west edge (edge 2) rises
        // linearly, and a's east edge (edge 3) follows it exactly — the even
        // samples coincide, the odd ones sit on the midpoints.
        for e in 0..4 {
            for i in (1..TERRAIN_PATCH_VERTS as usize).step_by(2) {
                put(&mut words, 1, e, i, 0.0, 0.0);
            }
        }
        for i in 0..TERRAIN_PATCH_VERTS as usize {
            if i % 2 == 0 {
                put(&mut words, 1, 2, i, 10.0 + i as f32, lip);
            }
            put(&mut words, 0, 3, i, 10.0 + i as f32, lip);
        }
        let stats = summarize_patch_readback(&[a, b], lip, &words);
        assert_eq!((stats.shared_edges, stats.level_edges, stats.coarse), (1, 1, 1));
        assert_eq!(stats.max_seam_gap, 0.0);
        assert_eq!(stats.max_lip_deviation, 0.0, "blank coarse samples are not lip errors");
        // An odd fine vertex left off the coarse edge is a crack.
        put(&mut words, 0, 3, 7, 17.5, lip);
        let stats = summarize_patch_readback(&[a, b], lip, &words);
        assert!((stats.max_seam_gap - 0.5).abs() < 1e-6, "{}", stats.max_seam_gap);
    }
}
