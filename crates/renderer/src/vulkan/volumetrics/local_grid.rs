//! World-space combustion local grid (tracking #6531, Phase 1 #6533).
//!
//! The froxel grid stores the transported fire in camera-aligned cells: close
//! to the camera a cell is a few centimetres across the screen but 0.63 m deep,
//! which is the lattice visible in the `s1_close_flame` baseline. This module
//! adds a second home for the same field: a few axis-aligned boxes of cubic
//! cells anchored in world space around the active emitters, simulated by
//! `volumetrics_local_sim.comp` with the transport shared with the froxel pass
//! (`include/combustion_transport.glsl`).
//!
//! **Stage 1b — simulated, not drawn.** Nothing reads the result yet; it is
//! reduced to a deterministic state hash (read back one frame-in-flight later)
//! so the pass can be verified and timed before the ray-march consumes it.
//!
//! The grid is **transient**: it is not part of a save (decided for #6533),
//! and is rebuilt from the `FogVolume` sources after a load.
//!
//! Boxes are stacked along Z in one atlas image per field (`state`,
//! `dynamics`, `optical`), double-buffered per frame in flight exactly like
//! the froxel fields.

use super::super::descriptors::{
    image_barrier_general_read_to_write, image_barrier_general_write_to_read,
};
use super::super::image::{GpuImage, GpuImageDesc};
use super::fog_clusters::{is_transport_emitter, GpuFogVolume};
use super::{
    write_combined_image_sampler, write_storage_buffer, write_storage_image, DescriptorPoolBuilder,
    GpuBuffer, SharedAllocator, TimerPass, COMBUSTION_FIELD_FORMAT, MAX_FRAMES_IN_FLIGHT,
    WORLD_UNITS_PER_METER,
};
use crate::shader_constants::{LOCAL_GRID_DIM, LOCAL_GRID_MAX_BOXES};
use crate::vulkan::descriptors::{
    clamp_sampler, compute_binding, general_sampled_image_info, storage_image_info,
};
use crate::vulkan::reflect::{validate_set_layout, ReflectedShader};
use anyhow::{Context, Result};
use ash::vk;

pub(crate) const VOLUMETRICS_LOCAL_SIM_COMP_SPV: &[u8] =
    include_bytes!("../../../shaders/volumetrics_local_sim.comp.spv");

/// Cells per box edge.
pub(crate) const LOCAL_GRID_CELLS: u32 = LOCAL_GRID_DIM;
/// Boxes that can be live at once.
pub(crate) const MAX_LOCAL_BOXES: usize = LOCAL_GRID_MAX_BOXES as usize;
/// Workgroup edge in `volumetrics_local_sim.comp`.
const LOCAL_SIM_WORKGROUP: u32 = 4;

/// Cell edges a box can be built with, in metres, finest first. A source gets
/// the finest one whose box still holds [`BOX_EDGE_PER_SOURCE_RADIUS`] source
/// radii; a source too large for the coarsest stays on the froxels (a nuclear
/// cloud, for instance, until Phase 6's burning-particle work).
pub(crate) const CELL_LADDER_METERS: [f32; 5] = [0.025, 0.05, 0.10, 0.20, 0.40];
/// Box edge, in source radii. Leaves headroom above the source for the plume.
pub(crate) const BOX_EDGE_PER_SOURCE_RADIUS: f32 = 6.0;
/// Fraction of the box edge kept clear of a contained source on every side but
/// the top, so the source never sits against the clamp.
const BOX_INNER_MARGIN: f32 = 0.10;
/// Box origins snap to this many cells so a source that moves a little keeps
/// its box, and so a re-created box lands on the same lattice.
const BOX_ORIGIN_SNAP_CELLS: f32 = 8.0;
/// Fraction of the box edge below the source's centre.
const BOX_BELOW_SOURCE: f32 = 0.30;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LocalBox {
    /// Absolute world position of the minimum corner.
    pub(crate) origin: [f32; 3],
    /// Cell edge, world units.
    pub(crate) cell: f32,
    /// Last simulation time a source was inside this box.
    pub(crate) last_source_time: f32,
    /// This box (same placement) was simulated last frame, so the previous
    /// frame-in-flight slot holds its field.
    pub(crate) history: bool,
}

impl LocalBox {
    pub(crate) fn edge(&self) -> f32 {
        self.cell * LOCAL_GRID_CELLS as f32
    }

    /// The source sphere sits inside the box with the inner margin clear on
    /// the sides and bottom (the top is free: that is where the plume goes).
    fn contains_source(&self, center: [f32; 3], radius: f32) -> bool {
        let edge = self.edge();
        let margin = BOX_INNER_MARGIN * edge;
        (0..3).all(|axis| {
            center[axis] - radius >= self.origin[axis] + margin
                && (axis == 1 || center[axis] + radius <= self.origin[axis] + edge - margin)
        }) && center[1] + radius <= self.origin[1] + edge
    }
}

/// The finest ladder cell whose box holds `radius_world` of source, or `None`
/// when even the coarsest cannot.
pub(crate) fn cell_for_source_radius(radius_world: f32) -> Option<f32> {
    CELL_LADDER_METERS
        .iter()
        .map(|meters| meters * WORLD_UNITS_PER_METER)
        .find(|cell| cell * LOCAL_GRID_CELLS as f32 >= BOX_EDGE_PER_SOURCE_RADIUS * radius_world)
}

/// Where a new box for a source of `radius` at `center` goes, snapped to a
/// lattice of `BOX_ORIGIN_SNAP_CELLS` cells.
fn place_box(center: [f32; 3], cell: f32) -> [f32; 3] {
    let edge = cell * LOCAL_GRID_CELLS as f32;
    let snap = cell * BOX_ORIGIN_SNAP_CELLS;
    let raw = [
        center[0] - 0.5 * edge,
        center[1] - BOX_BELOW_SOURCE * edge,
        center[2] - 0.5 * edge,
    ];
    raw.map(|v| (v / snap).floor() * snap)
}

/// Host-side bookkeeping of which slots of the atlas hold which box.
///
/// Persistent across frames so a box keeps its field; a source that no box
/// holds gets a free slot, and a box with no source for the linger window
/// is released.
#[derive(Default)]
pub(crate) struct LocalBoxSet {
    boxes: [Option<LocalBox>; MAX_LOCAL_BOXES],
}

impl LocalBoxSet {
    pub(crate) fn boxes(&self) -> &[Option<LocalBox>; MAX_LOCAL_BOXES] {
        &self.boxes
    }

    pub(crate) fn live_count(&self) -> usize {
        self.boxes.iter().flatten().count()
    }

    /// One past the highest live slot — the number of atlas slabs a dispatch
    /// has to cover.
    pub(crate) fn slab_count(&self) -> usize {
        self.boxes
            .iter()
            .rposition(Option::is_some)
            .map_or(0, |last| last + 1)
    }

    pub(crate) fn live_mask(&self) -> u32 {
        self.boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.is_some())
            .fold(0, |mask, (slot, _)| mask | (1 << slot))
    }

    pub(crate) fn history_mask(&self) -> u32 {
        self.boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.is_some_and(|b| b.history))
            .fold(0, |mask, (slot, _)| mask | (1 << slot))
    }

    /// Drop every box and its history (cell change, device reset).
    pub(crate) fn clear(&mut self) {
        self.boxes = Default::default();
    }

    /// Keep the placements but forget that any of them was simulated, so the
    /// next frame re-seeds from the sources instead of reading a stale slot.
    pub(crate) fn invalidate_history(&mut self) {
        for b in self.boxes.iter_mut().flatten() {
            b.history = false;
        }
    }

    /// Called once the frame that simulated the current boxes was submitted.
    pub(crate) fn commit_history(&mut self) {
        for b in self.boxes.iter_mut().flatten() {
            b.history = true;
        }
    }

    /// Match every transported source to a box, create boxes for the ones left
    /// over (nearest to the camera first, while slots remain) and release
    /// boxes that have had no source for `linger_seconds`.
    ///
    /// `reach_world` is the half edge of the camera-centred cluster cube the
    /// shader's source lookup (`localFogCluster`) is limited to; a source
    /// outside it cannot be simulated here and is left to the froxels.
    pub(crate) fn update(
        &mut self,
        volumes: &[GpuFogVolume],
        camera: [f32; 3],
        reach_world: f32,
        now: f32,
        linger_seconds: f32,
    ) {
        struct Source {
            center: [f32; 3],
            radius: f32,
            distance_sq: f32,
        }
        let mut sources: Vec<Source> = volumes
            .iter()
            .filter(|volume| is_transport_emitter(volume))
            .filter_map(|volume| {
                let center = [
                    volume.center_shape[0],
                    volume.center_shape[1],
                    volume.center_shape[2],
                ];
                let radius = volume.half_extents_extinction[..3]
                    .iter()
                    .fold(0.0f32, |acc, v| acc.max(*v));
                let distance_sq: f32 = (0..3).map(|a| (center[a] - camera[a]).powi(2)).sum();
                (center.iter().all(|v| v.is_finite())
                    && radius.is_finite()
                    && radius > 0.0
                    && distance_sq.sqrt() <= reach_world)
                    .then_some(Source {
                        center,
                        radius,
                        distance_sq,
                    })
            })
            .collect();
        sources.sort_by(|a, b| a.distance_sq.total_cmp(&b.distance_sq));

        for source in &sources {
            let Some(wanted) = cell_for_source_radius(source.radius) else {
                continue;
            };
            if let Some(held) = self.boxes.iter_mut().flatten().find(|b| {
                b.cell <= wanted * 1.0001 && b.contains_source(source.center, source.radius)
            }) {
                held.last_source_time = now;
                continue;
            }
            if let Some(slot) = self.boxes.iter().position(Option::is_none) {
                self.boxes[slot] = Some(LocalBox {
                    origin: place_box(source.center, wanted),
                    cell: wanted,
                    last_source_time: now,
                    history: false,
                });
            }
        }

        for slot in self.boxes.iter_mut() {
            if slot.is_some_and(|b| now - b.last_source_time > linger_seconds) {
                *slot = None;
            }
        }
    }
}

/// Push constants of `volumetrics_local_sim.comp`: 4 × `vec4` box (xyz origin,
/// w cell) then `uvec4` info (x slab count, y history mask, z live mask).
pub(crate) const LOCAL_SIM_PUSH_BYTES: usize = MAX_LOCAL_BOXES * 16 + 16;

pub(crate) fn local_sim_push_bytes(boxes: &LocalBoxSet) -> [u8; LOCAL_SIM_PUSH_BYTES] {
    let mut bytes = [0u8; LOCAL_SIM_PUSH_BYTES];
    let mut put = |offset: usize, word: [u8; 4]| bytes[offset..offset + 4].copy_from_slice(&word);
    for (slot, b) in boxes.boxes().iter().enumerate() {
        if let Some(b) = b {
            for axis in 0..3 {
                put(slot * 16 + axis * 4, b.origin[axis].to_ne_bytes());
            }
            put(slot * 16 + 12, b.cell.to_ne_bytes());
        }
    }
    let info = MAX_LOCAL_BOXES * 16;
    put(info, (boxes.slab_count() as u32).to_ne_bytes());
    put(info + 4, boxes.history_mask().to_ne_bytes());
    put(info + 8, boxes.live_mask().to_ne_bytes());
    bytes
}

/// What the sim pass reports about itself, summed over boxes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LocalGridStats {
    pub(crate) boxes: u32,
    /// Cells that carry combustion.
    pub(crate) active_cells: u32,
    /// Order-independent hash of every active cell's state.
    pub(crate) state_hash: u32,
}

const STATS_WORDS_PER_BOX: usize = 4;
const STATS_BYTES: usize = MAX_LOCAL_BOXES * STATS_WORDS_PER_BOX * 4;

fn fold_stats(
    words: &[u32; MAX_LOCAL_BOXES * STATS_WORDS_PER_BOX],
    live_mask: u32,
) -> LocalGridStats {
    let mut stats = LocalGridStats::default();
    for slot in 0..MAX_LOCAL_BOXES {
        if live_mask & (1 << slot) == 0 {
            continue;
        }
        let w = &words[slot * STATS_WORDS_PER_BOX..][..STATS_WORDS_PER_BOX];
        stats.boxes += 1;
        stats.active_cells += w[0];
        stats.state_hash =
            stats.state_hash.wrapping_mul(0x9e37_79b1).rotate_left(5) ^ w[1] ^ w[2].rotate_left(16);
    }
    stats
}

pub(crate) fn local_grid_layout_bindings() -> Vec<vk::DescriptorSetLayoutBinding<'static>> {
    vec![
        // 0..2: this frame's state / dynamics / optical atlas (write).
        compute_binding(0, vk::DescriptorType::STORAGE_IMAGE),
        compute_binding(1, vk::DescriptorType::STORAGE_IMAGE),
        compute_binding(2, vk::DescriptorType::STORAGE_IMAGE),
        // 3..5: the previous frame-in-flight slot of each (sampled).
        compute_binding(3, vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
        compute_binding(4, vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
        compute_binding(5, vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
        // 6: per-box state reduction, drained by the host a frame later.
        compute_binding(6, vk::DescriptorType::STORAGE_BUFFER),
    ]
}

/// The atlas extent: one box wide and high, `MAX_LOCAL_BOXES` boxes deep.
fn atlas_extent() -> vk::Extent3D {
    vk::Extent3D {
        width: LOCAL_GRID_CELLS,
        height: LOCAL_GRID_CELLS,
        depth: LOCAL_GRID_CELLS * MAX_LOCAL_BOXES as u32,
    }
}

/// VRAM of the atlas fields across every frame in flight: three RGBA16F
/// volumes (8 B per cell) per slot.
pub(crate) const fn local_grid_resident_bytes() -> u64 {
    let cells = (LOCAL_GRID_CELLS as u64).pow(2) * LOCAL_GRID_CELLS as u64 * MAX_LOCAL_BOXES as u64;
    cells * 8 * 3 * MAX_FRAMES_IN_FLIGHT as u64
}

pub(crate) struct LocalGrid {
    set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    pool: vk::DescriptorPool,
    sets: Vec<vk::DescriptorSet>,
    sampler: vk::Sampler,
    state: Vec<GpuImage>,
    dynamics: Vec<GpuImage>,
    optical: Vec<GpuImage>,
    stats_buffers: Vec<GpuBuffer>,
    /// The live mask each slot's stats were recorded under.
    stats_live_mask: [u32; MAX_FRAMES_IN_FLIGHT],
    /// A dispatch has written this slot's stats since the last drain.
    stats_pending: [bool; MAX_FRAMES_IN_FLIGHT],
    last_stats: LocalGridStats,
    boxes: LocalBoxSet,
    dispatched_this_frame: bool,
}

impl LocalGrid {
    /// `inject_set_layout` is the froxel inject pass's set 0 layout; this pass
    /// binds the same descriptor set, so the layouts must be the same object.
    pub(crate) fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        inject_set_layout: vk::DescriptorSetLayout,
    ) -> Result<Self> {
        let mut grid = Self {
            set_layout: vk::DescriptorSetLayout::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            pool: vk::DescriptorPool::null(),
            sets: Vec::new(),
            sampler: vk::Sampler::null(),
            state: Vec::new(),
            dynamics: Vec::new(),
            optical: Vec::new(),
            stats_buffers: Vec::new(),
            stats_live_mask: [0; MAX_FRAMES_IN_FLIGHT],
            stats_pending: [false; MAX_FRAMES_IN_FLIGHT],
            last_stats: LocalGridStats::default(),
            boxes: LocalBoxSet::default(),
            dispatched_this_frame: false,
        };
        match grid.build(device, allocator, pipeline_cache, inject_set_layout) {
            Ok(()) => Ok(grid),
            Err(e) => {
                // SAFETY: nothing built here has been submitted to the GPU.
                unsafe { grid.destroy(device, allocator) };
                Err(e)
            }
        }
    }

    fn build(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        inject_set_layout: vk::DescriptorSetLayout,
    ) -> Result<()> {
        let bindings = local_grid_layout_bindings();
        validate_set_layout(
            1,
            &bindings,
            &[ReflectedShader {
                name: "volumetrics_local_sim.comp",
                spirv: VOLUMETRICS_LOCAL_SIM_COMP_SPV,
            }],
            "volumetrics_local_grid",
            &[],
        )
        .context("local grid layout drifted against volumetrics_local_sim.comp")?;

        // SAFETY: `device` is live; `bindings` outlives the call; the handle is
        // owned by `self` and destroyed in `destroy`.
        self.set_layout = unsafe {
            device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )
        }
        .context("local grid descriptor set layout")?;

        let push_range = vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::COMPUTE,
            offset: 0,
            size: LOCAL_SIM_PUSH_BYTES as u32,
        };
        let set_layouts = [inject_set_layout, self.set_layout];
        // SAFETY: both set layouts are live; `push_range` outlives the call.
        self.pipeline_layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&set_layouts)
                    .push_constant_ranges(std::slice::from_ref(&push_range)),
                None,
            )
        }
        .context("local grid pipeline layout")?;

        self.pipeline = super::super::pipeline::create_compute_pipeline(
            device,
            pipeline_cache,
            VOLUMETRICS_LOCAL_SIM_COMP_SPV,
            self.pipeline_layout,
            "Volumetrics local grid sim",
        )?;

        // SAFETY: `device` is live; handle owned by `self`.
        self.sampler =
            unsafe { clamp_sampler(device, vk::Filter::LINEAR, vk::SamplerMipmapMode::NEAREST) }
                .context("local grid sampler")?;

        let usage = vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_DST;
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            for (name, fields) in [
                ("state", &mut self.state),
                ("dynamics", &mut self.dynamics),
                ("optical", &mut self.optical),
            ] {
                let image = GpuImage::create(
                    device,
                    allocator,
                    &GpuImageDesc::color_3d(
                        &format!("volumetrics_local_{name}_{i}"),
                        atlas_extent(),
                        COMBUSTION_FIELD_FORMAT,
                        usage,
                    ),
                )?;
                fields.push(image);
            }
            let mut stats = GpuBuffer::create_host_readback(
                device,
                allocator,
                STATS_BYTES as vk::DeviceSize,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?;
            // Pushed before it is zeroed, so a failed write is cleaned up by `destroy`.
            let zeroed = (|| -> Result<()> {
                stats.mapped_slice_mut()?.fill(0);
                stats.flush_if_needed(device)
            })();
            self.stats_buffers.push(stats);
            zeroed?;
        }

        self.pool =
            DescriptorPoolBuilder::from_layout_bindings(&bindings, MAX_FRAMES_IN_FLIGHT as u32)
                .max_sets(MAX_FRAMES_IN_FLIGHT as u32)
                .build(device, "Volumetrics local grid descriptor pool")?;
        let layouts = vec![self.set_layout; MAX_FRAMES_IN_FLIGHT];
        // SAFETY: the pool was just built and `layouts` outlives the call.
        self.sets = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(self.pool)
                    .set_layouts(&layouts),
            )
        }
        .context("local grid descriptor sets")?;

        for f in 0..MAX_FRAMES_IN_FLIGHT {
            let previous = (f + MAX_FRAMES_IN_FLIGHT - 1) % MAX_FRAMES_IN_FLIGHT;
            let write_state = [storage_image_info(self.state[f].view)];
            let write_dynamics = [storage_image_info(self.dynamics[f].view)];
            let write_optical = [storage_image_info(self.optical[f].view)];
            let read_state = [general_sampled_image_info(
                self.sampler,
                self.state[previous].view,
            )];
            let read_dynamics = [general_sampled_image_info(
                self.sampler,
                self.dynamics[previous].view,
            )];
            let read_optical = [general_sampled_image_info(
                self.sampler,
                self.optical[previous].view,
            )];
            let stats_info = [vk::DescriptorBufferInfo {
                buffer: self.stats_buffers[f].buffer,
                offset: 0,
                range: STATS_BYTES as vk::DeviceSize,
            }];
            let set = self.sets[f];
            let writes = [
                write_storage_image(set, 0, &write_state),
                write_storage_image(set, 1, &write_dynamics),
                write_storage_image(set, 2, &write_optical),
                write_combined_image_sampler(set, 3, &read_state),
                write_combined_image_sampler(set, 4, &read_dynamics),
                write_combined_image_sampler(set, 5, &read_optical),
                write_storage_buffer(set, 6, &stats_info),
            ];
            // SAFETY: the sets are freshly allocated and not yet in use.
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }
        log::info!(
            "Volumetrics local grid created: {} boxes of {}^3 cells, {} MiB resident (3 x RGBA16F x {} slots)",
            MAX_LOCAL_BOXES,
            LOCAL_GRID_CELLS,
            local_grid_resident_bytes() / (1024 * 1024),
            MAX_FRAMES_IN_FLIGHT,
        );
        Ok(())
    }

    /// Every atlas image, for the one-time layout transition and clear.
    pub(crate) fn images(&self) -> impl Iterator<Item = &GpuImage> {
        self.state
            .iter()
            .chain(self.dynamics.iter())
            .chain(self.optical.iter())
    }

    pub(crate) fn stats(&self) -> LocalGridStats {
        self.last_stats
    }

    /// Placement for this frame. Call before the frame's host-to-compute
    /// barrier so the drained, re-zeroed stats buffer is covered by it.
    ///
    /// Also drains this slot's stats from the dispatch that last used it; the
    /// caller must have waited that slot's fence (the all-slots wait at the
    /// top of `draw_frame`).
    pub(crate) fn begin_frame(
        &mut self,
        device: &ash::Device,
        frame: usize,
        volumes: &[GpuFogVolume],
        camera: [f32; 3],
        reach_world: f32,
        now: f32,
        linger_seconds: f32,
    ) -> Result<()> {
        if std::mem::take(&mut self.stats_pending[frame]) {
            let buffer = &mut self.stats_buffers[frame];
            buffer.invalidate_if_needed(device)?;
            let mut words = [0u32; MAX_LOCAL_BOXES * STATS_WORDS_PER_BOX];
            {
                let bytes = buffer.mapped_slice_mut()?;
                for (word, chunk) in words.iter_mut().zip(bytes.chunks_exact(4)) {
                    *word = u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                }
                bytes[..STATS_BYTES].fill(0);
            }
            buffer.flush_if_needed(device)?;
            self.last_stats = fold_stats(&words, self.stats_live_mask[frame]);
        }
        self.boxes
            .update(volumes, camera, reach_world, now, linger_seconds);
        Ok(())
    }

    pub(crate) fn has_work(&self) -> bool {
        self.boxes.live_count() > 0
    }

    /// Record the sim pass for every live box.
    ///
    /// # Safety
    ///
    /// `cmd` must be recording outside a render pass, `frame` must name an
    /// idle frame-in-flight slot, and `inject_set` must be that slot's
    /// froxel-inject descriptor set with its per-frame bindings written.
    pub(crate) unsafe fn record_sim(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        inject_set: vk::DescriptorSet,
        mut timers: Option<&mut super::super::gpu_timers::GpuPerFrameTimers>,
    ) {
        let previous = (frame + MAX_FRAMES_IN_FLIGHT - 1) % MAX_FRAMES_IN_FLIGHT;
        let barriers = [&self.state, &self.dynamics, &self.optical].map(|slots| {
            [
                image_barrier_general_read_to_write(slots[frame].image),
                image_barrier_general_write_to_read(slots[previous].image),
            ]
        });
        // SAFETY: caller contract — `cmd` is recording and the images are the
        // pipeline's own, living in GENERAL (`initialize_layouts`). The
        // barriers sequence this slot's last reads (two frames ago) before
        // this write and the previous slot's write before this read; the
        // execution dependency also keeps the froxel inject dispatch recorded
        // just before from overlapping this bracket.
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                barriers.as_flattened(),
            );
        }
        if let Some(timers) = timers.as_deref_mut() {
            timers.cmd_start(TimerPass::VolumetricsLocalSim, device, cmd, frame);
        }
        let push = local_sim_push_bytes(&self.boxes);
        let groups = LOCAL_GRID_CELLS / LOCAL_SIM_WORKGROUP;
        // SAFETY: pipeline, layout and sets are live and compatible (set 0 is
        // the inject layout this pipeline layout was built from); the push
        // range matches the shader's block.
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline_layout,
                0,
                &[inject_set, self.sets[frame]],
                &[],
            );
            device.cmd_push_constants(
                cmd,
                self.pipeline_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                &push,
            );
            device.cmd_dispatch(cmd, groups, groups, groups * self.boxes.slab_count() as u32);
        }
        if let Some(timers) = timers {
            timers.cmd_end(TimerPass::VolumetricsLocalSim, device, cmd, frame);
        }
        self.stats_live_mask[frame] = self.boxes.live_mask();
        self.stats_pending[frame] = true;
        self.dispatched_this_frame = true;
    }

    /// The frame that recorded `record_sim` was submitted.
    pub(crate) fn mark_frame_completed(&mut self) {
        if std::mem::take(&mut self.dispatched_this_frame) {
            self.boxes.commit_history();
        }
    }

    pub(crate) fn invalidate_history(&mut self) {
        self.dispatched_this_frame = false;
        self.boxes.invalidate_history();
    }

    pub(crate) fn reset(&mut self) {
        self.dispatched_this_frame = false;
        self.boxes.clear();
        self.last_stats = LocalGridStats::default();
    }

    /// # Safety
    ///
    /// The GPU must be done with every resource (device idle or all slots
    /// fenced), as for `VolumetricsPipeline::destroy`.
    pub(crate) unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for mut image in self
            .state
            .drain(..)
            .chain(self.dynamics.drain(..))
            .chain(self.optical.drain(..))
        {
            image.destroy(device, allocator);
        }
        for buffer in &mut self.stats_buffers {
            buffer.destroy(device, allocator);
        }
        self.stats_buffers.clear();
        // SAFETY: caller contract — nothing is in flight; each handle is
        // destroyed once and nulled.
        unsafe {
            if self.pool != vk::DescriptorPool::null() {
                device.destroy_descriptor_pool(self.pool, None);
                self.pool = vk::DescriptorPool::null();
            }
            self.sets.clear();
            if self.pipeline != vk::Pipeline::null() {
                device.destroy_pipeline(self.pipeline, None);
                self.pipeline = vk::Pipeline::null();
            }
            if self.pipeline_layout != vk::PipelineLayout::null() {
                device.destroy_pipeline_layout(self.pipeline_layout, None);
                self.pipeline_layout = vk::PipelineLayout::null();
            }
            if self.set_layout != vk::DescriptorSetLayout::null() {
                device.destroy_descriptor_set_layout(self.set_layout, None);
                self.set_layout = vk::DescriptorSetLayout::null();
            }
            if self.sampler != vk::Sampler::null() {
                device.destroy_sampler(self.sampler, None);
                self.sampler = vk::Sampler::null();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shader_constants::{FOG_VOLUME_PROFILE_FLAME, FOG_VOLUME_PROFILE_HOMOGENEOUS};

    const METER: f32 = WORLD_UNITS_PER_METER;

    fn flame(center_m: [f32; 3], radius_m: f32) -> GpuFogVolume {
        let mut v = GpuFogVolume::default();
        v.center_shape = [
            center_m[0] * METER,
            center_m[1] * METER,
            center_m[2] * METER,
            0.0,
        ];
        v.half_extents_extinction = [radius_m * METER, radius_m * METER, radius_m * METER, 1.0];
        v.profile_params[0] = FOG_VOLUME_PROFILE_FLAME as f32;
        v
    }

    fn update(set: &mut LocalBoxSet, volumes: &[GpuFogVolume], now: f32) {
        set.update(volumes, [0.0; 3], 128.0 * METER, now, 10.0);
    }

    #[test]
    fn a_torch_gets_the_finest_cell_and_a_campfire_the_next() {
        // 6 radii must fit the 64-cell box: 0.15 m -> 0.9 m <= 1.6 m (2.5 cm);
        // 0.4 m -> 2.4 m > 1.6 m but <= 3.2 m (5 cm).
        assert_eq!(cell_for_source_radius(0.15 * METER), Some(0.025 * METER));
        assert_eq!(cell_for_source_radius(0.4 * METER), Some(0.05 * METER));
    }

    #[test]
    fn a_source_too_large_for_the_coarsest_cell_stays_on_the_froxels() {
        let coarsest = CELL_LADDER_METERS[CELL_LADDER_METERS.len() - 1];
        let largest_radius = coarsest * LOCAL_GRID_CELLS as f32 / BOX_EDGE_PER_SOURCE_RADIUS;
        assert!(cell_for_source_radius(largest_radius * 0.999 * METER).is_some());
        assert_eq!(cell_for_source_radius(largest_radius * 1.01 * METER), None);
        let mut set = LocalBoxSet::default();
        update(
            &mut set,
            &[flame([1.0, 0.0, 1.0], largest_radius * 1.5)],
            0.0,
        );
        assert_eq!(set.live_count(), 0);
    }

    #[test]
    fn a_new_source_gets_a_box_that_contains_it_with_no_history() {
        let mut set = LocalBoxSet::default();
        update(&mut set, &[flame([2.0, 0.5, 3.0], 0.3)], 0.0);
        assert_eq!(set.live_count(), 1);
        let b = set.boxes()[0].expect("slot 0");
        assert!(!b.history);
        assert!(b.contains_source([2.0 * METER, 0.5 * METER, 3.0 * METER], 0.3 * METER));
        assert_eq!(set.history_mask(), 0);
        assert_eq!(set.live_mask(), 1);
        assert_eq!(set.slab_count(), 1);
    }

    #[test]
    fn a_held_source_keeps_its_box_and_origin_across_frames() {
        let mut set = LocalBoxSet::default();
        let fire = [flame([2.0, 0.5, 3.0], 0.3)];
        update(&mut set, &fire, 0.0);
        set.commit_history();
        let first = set.boxes()[0].unwrap();
        update(&mut set, &fire, 1.0 / 60.0);
        let second = set.boxes()[0].unwrap();
        assert_eq!(set.live_count(), 1);
        assert_eq!(first.origin, second.origin);
        assert!(second.history, "an unchanged placement keeps its history");
    }

    #[test]
    fn two_flames_inside_one_box_share_it() {
        let mut set = LocalBoxSet::default();
        // 5 cm cells -> 3.2 m box. Two 0.3 m flames 0.3 m apart fit together.
        update(
            &mut set,
            &[flame([2.0, 0.5, 3.0], 0.3), flame([2.3, 0.5, 3.0], 0.3)],
            0.0,
        );
        assert_eq!(set.live_count(), 1);
    }

    #[test]
    fn distant_fires_get_separate_boxes_nearest_first_until_slots_run_out() {
        let mut set = LocalBoxSet::default();
        let fires: Vec<_> = (0..MAX_LOCAL_BOXES as i32 + 2)
            .map(|i| flame([10.0 * (i + 1) as f32, 0.5, 0.0], 0.3))
            .collect();
        update(&mut set, &fires, 0.0);
        assert_eq!(set.live_count(), MAX_LOCAL_BOXES);
        // The nearest MAX fires are the ones simulated; the farthest two are not.
        let farthest = fires.last().unwrap().center_shape;
        assert!(!set
            .boxes()
            .iter()
            .flatten()
            .any(|b| b.contains_source([farthest[0], farthest[1], farthest[2]], 0.3 * METER)));
    }

    #[test]
    fn passive_fog_and_out_of_reach_sources_get_no_box() {
        let mut set = LocalBoxSet::default();
        let mut fog = flame([2.0, 0.5, 3.0], 0.3);
        fog.profile_params[0] = FOG_VOLUME_PROFILE_HOMOGENEOUS as f32;
        update(&mut set, &[fog, flame([500.0, 0.0, 0.0], 0.3)], 0.0);
        assert_eq!(set.live_count(), 0);
    }

    #[test]
    fn a_box_outlives_its_source_by_the_linger_window_then_frees_its_slot() {
        let mut set = LocalBoxSet::default();
        update(&mut set, &[flame([2.0, 0.5, 3.0], 0.3)], 0.0);
        update(&mut set, &[], 9.9);
        assert_eq!(set.live_count(), 1, "still inside the 10 s linger");
        update(&mut set, &[], 10.1);
        assert_eq!(set.live_count(), 0);
        assert_eq!(set.slab_count(), 0);
    }

    #[test]
    fn invalidating_history_keeps_placements_but_not_history() {
        let mut set = LocalBoxSet::default();
        update(&mut set, &[flame([2.0, 0.5, 3.0], 0.3)], 0.0);
        set.commit_history();
        assert_eq!(set.history_mask(), 1);
        set.invalidate_history();
        assert_eq!(set.history_mask(), 0);
        assert_eq!(set.live_count(), 1);
        set.clear();
        assert_eq!(set.live_count(), 0);
    }

    #[test]
    fn slab_count_covers_the_highest_live_slot_even_with_a_gap() {
        let mut set = LocalBoxSet::default();
        let fires = [
            flame([10.0, 0.5, 0.0], 0.3),
            flame([20.0, 0.5, 0.0], 0.3),
            flame([30.0, 0.5, 0.0], 0.3),
        ];
        update(&mut set, &fires, 0.0);
        assert_eq!(set.live_mask(), 0b111);
        // Slot 0's fire goes out for good; slots 1 and 2 stay lit.
        let mut now = 0.0;
        while set.boxes()[0].is_some() {
            now += 1.0;
            update(&mut set, &fires[1..], now);
        }
        assert_eq!(set.live_mask(), 0b110);
        assert_eq!(
            set.slab_count(),
            3,
            "the dispatch still has to reach slot 2"
        );
    }

    #[test]
    fn push_constants_encode_boxes_then_counts_in_shader_order() {
        let mut set = LocalBoxSet::default();
        update(&mut set, &[flame([2.0, 0.5, 3.0], 0.3)], 0.0);
        set.commit_history();
        let b = set.boxes()[0].unwrap();
        let bytes = local_sim_push_bytes(&set);
        let f = |o: usize| f32::from_ne_bytes(bytes[o..o + 4].try_into().unwrap());
        let u = |o: usize| u32::from_ne_bytes(bytes[o..o + 4].try_into().unwrap());
        assert_eq!(
            [f(0), f(4), f(8), f(12)],
            [b.origin[0], b.origin[1], b.origin[2], b.cell]
        );
        let info = MAX_LOCAL_BOXES * 16;
        assert_eq!([u(info), u(info + 4), u(info + 8)], [1, 1, 1]);
        assert_eq!(
            LOCAL_SIM_PUSH_BYTES, 80,
            "within the 128 B guaranteed push range"
        );
    }

    #[test]
    fn push_block_in_the_shader_matches_the_encoder() {
        let shader = include_str!("../../../shaders/volumetrics_local_sim.comp");
        assert!(shader.contains("vec4 boxes[LOCAL_GRID_MAX_BOXES];"));
        assert!(shader.contains("uvec4 info;"));
        let boxes = shader.find("vec4 boxes[").unwrap();
        let info = shader.find("uvec4 info;").unwrap();
        assert!(boxes < info, "boxes precede info, as encoded");
        for field in ["push.info.x", "push.info.y", "push.info.z"] {
            assert!(shader.contains(field), "shader no longer reads {field}");
        }
        // The shader's atlas depth and the host's dispatch/atlas sizing share
        // the generated constants, never literals.
        assert!(shader.contains("LOCAL_GRID_DIM") && shader.contains("LOCAL_GRID_MAX_BOXES"));
    }

    #[test]
    fn stats_fold_ignores_dead_slots_and_is_order_sensitive_only_by_slot() {
        let mut words = [0u32; MAX_LOCAL_BOXES * STATS_WORDS_PER_BOX];
        words[0] = 5;
        words[1] = 0xAAAA;
        words[2] = 0x1111;
        words[STATS_WORDS_PER_BOX] = 99; // slot 1, dead: must not count
        let stats = fold_stats(&words, 0b01);
        assert_eq!((stats.boxes, stats.active_cells), (1, 5));
        assert_eq!(fold_stats(&words, 0b01), stats, "pure function");
        assert_ne!(fold_stats(&words, 0b11).active_cells, stats.active_cells);
    }

    #[test]
    fn memory_budget_doc_states_the_resident_cost() {
        const DOC: &str = include_str!("../../../../../docs/engine/memory-budget.md");
        let mib = local_grid_resident_bytes() / (1024 * 1024);
        assert!(
            DOC.contains(&format!("**{mib} MiB resident**")),
            "memory-budget.md must state the local grid's {mib} MiB (re-derive the ledger row)"
        );
    }

    #[test]
    fn resident_vram_is_small_enough_for_the_budget() {
        // 4 boxes of 64^3 cells, three RGBA16F fields, two slots.
        assert_eq!(local_grid_resident_bytes(), 64 * 64 * 64 * 4 * 8 * 3 * 2);
        assert!(local_grid_resident_bytes() < 64 * 1024 * 1024);
    }
}
