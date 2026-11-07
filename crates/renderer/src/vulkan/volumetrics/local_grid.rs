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
//! **Stage 1c — simulated and drawn.** Composite's close-range fire march reads
//! the atlas, the irradiance field, the occupancy bricks and the phase table
//! (`LocalFireViews`, set 0 bindings 12..=17). The pass is also reduced to a
//! deterministic state hash (read back one frame-in-flight later) so it can
//! be verified and timed independently of what draws it.
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
    write_combined_image_sampler, write_storage_buffer, write_storage_image, write_uniform_buffer,
    DescriptorPoolBuilder, GpuBuffer, SharedAllocator, TimerPass, VolumetricsParams,
    COMBUSTION_FIELD_FORMAT, FROXEL_FORMAT, MAX_FRAMES_IN_FLIGHT, WORLD_UNITS_PER_METER,
};
use crate::shader_constants::{
    LOCAL_GRID_BRICK, FOG_VOLUME_PROFILE_EXPLOSION, FOG_VOLUME_PROFILE_EXPLOSION_NUCLEAR,
    FOG_VOLUME_PROFILE_EXPLOSION_OIL, LOCAL_GRID_DEBUG_DENSE_MARCH, LOCAL_GRID_DEBUG_NO_HANDOFF, LOCAL_GRID_DEBUG_NO_SPECIES, LOCAL_GRID_DEBUG_NO_MARCH,
    COMBUSTION_PHASE_TABLE_WIDTH, LOCAL_GRID_DIM, LOCAL_GRID_MAX_BOXES,
};
use crate::vulkan::descriptors::{
    clamp_sampler, compute_binding, general_sampled_image_info, storage_image_info,
};
use crate::vulkan::reflect::{validate_set_layout, ReflectedShader};
use anyhow::{Context, Result};
use ash::vk;

pub(crate) const VOLUMETRICS_LOCAL_SIM_COMP_SPV: &[u8] =
    include_bytes!("../../../shaders/volumetrics_local_sim.comp.spv");
/// Evaluates the species phase lobes into [`LocalGrid`]'s phase table, once.
const COMBUSTION_PHASE_BAKE_COMP_SPV: &[u8] =
    include_bytes!("../../../shaders/combustion_phase_bake.comp.spv");
/// Workgroup width in `combustion_phase_bake.comp`.
const PHASE_BAKE_WORKGROUP: u32 = 64;
/// The table's rows: soot, then steam.
const PHASE_TABLE_ROWS: u32 = 2;

/// Cells per box edge.
pub(crate) const LOCAL_GRID_CELLS: u32 = LOCAL_GRID_DIM;
/// Boxes that can be live at once.
pub(crate) const MAX_LOCAL_BOXES: usize = LOCAL_GRID_MAX_BOXES as usize;
/// Bytes of one slot's occupancy-brick buffer: one `uint` per brick, per box
/// `(LOCAL_GRID_DIM / LOCAL_GRID_BRICK)^3` bricks.
pub(crate) const BRICK_BUFFER_BYTES: u64 = (MAX_LOCAL_BOXES as u64)
    * ((LOCAL_GRID_DIM / LOCAL_GRID_BRICK) as u64).pow(3)
    * 4;
/// Workgroup edge in `volumetrics_local_sim.comp`.
const LOCAL_SIM_WORKGROUP: u32 = 4;

/// Cell edges a box can be built with, in metres, finest first. A source gets
/// the finest one whose box still holds [`BOX_EDGE_PER_SOURCE_RADIUS`] source
/// radii; a source too large for the coarsest stays on the froxels (a nuclear
/// cloud, for instance, until Phase 6's burning-particle work).
pub(crate) const CELL_LADDER_METERS: [f32; 5] = [0.025, 0.05, 0.10, 0.20, 0.40];
/// Fires farther than this stay on the froxels (#6533: "distant fires
/// unchanged"). A flame this far is a few screen pixels wide, and the froxel
/// slices there are fine enough that the lattice no longer shows; spending a
/// 64^3 box and a march on it buys nothing.
pub(crate) const LOCAL_GRID_REACH_METERS: f32 = 20.0;
/// A box whose source has drifted out to this multiple of the reach is kept
/// (so a fire at the edge of the range does not lose its field every time the
/// camera steps back and forth); only new boxes are held to the plain reach.
const LOCAL_GRID_REACH_HYSTERESIS: f32 = 1.25;
/// Box edge, in source radii. Leaves headroom above the source for the plume.
pub(crate) const BOX_EDGE_PER_SOURCE_RADIUS: f32 = 6.0;
/// Box edge, in source radii, for an explosion. An explosion's `FogVolume` is
/// the swept bound of the authored effect, but the gas it drives is pushed out
/// by the blast far beyond it (the lab's nuclear source is 0.72 m across and the
/// dome reaches several metres), and a box the gas outgrows cuts the dome flat
/// at its faces. So explosions get a box sized for where the gas goes, not
/// where it starts.
pub(crate) const BOX_EDGE_PER_EXPLOSION_RADIUS: f32 = 16.0;
/// Fraction of the box edge below an explosion's centre: a blast expands in
/// every direction, so it is centred rather than given the flame's headroom.
const BOX_BELOW_EXPLOSION: f32 = 0.5;
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
    /// Built for an explosion. Such a box is sized for where the blast drives
    /// its gas, holds every source inside it whatever their own cell size, and
    /// replaces any finer box it overlaps: two boxes that overlap simulate
    /// independent fluids, and the finer one owns the shared space in the draw,
    /// so the blast's gas would be missing inside it (a rectangular hole in the
    /// cloud).
    pub(crate) explosion: bool,
}

impl LocalBox {
    pub(crate) fn edge(&self) -> f32 {
        self.cell * LOCAL_GRID_CELLS as f32
    }

    /// The two boxes' volumes overlap.
    fn overlaps(&self, other: &LocalBox) -> bool {
        (0..3).all(|axis| {
            self.origin[axis] < other.origin[axis] + other.edge()
                && other.origin[axis] < self.origin[axis] + self.edge()
        })
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

/// Whether `volume` is an explosion (any profile) rather than a flame or smoke.
fn is_explosion(volume: &GpuFogVolume) -> bool {
    let profile = volume.profile_params[0];
    [
        FOG_VOLUME_PROFILE_EXPLOSION,
        FOG_VOLUME_PROFILE_EXPLOSION_OIL,
        FOG_VOLUME_PROFILE_EXPLOSION_NUCLEAR,
    ]
    .iter()
    .any(|kind| (profile - kind).abs() < 0.5)
}

/// The finest ladder cell whose box holds `radius_world` of source, or `None`
/// when even the coarsest cannot.
pub(crate) fn cell_for_source_radius(radius_world: f32) -> Option<f32> {
    cell_for_edge(BOX_EDGE_PER_SOURCE_RADIUS * radius_world)
}

/// The finest ladder cell whose box is at least `edge_world` across.
fn cell_for_edge(edge_world: f32) -> Option<f32> {
    CELL_LADDER_METERS
        .iter()
        .map(|meters| meters * WORLD_UNITS_PER_METER)
        .find(|cell| cell * LOCAL_GRID_CELLS as f32 >= edge_world)
}

/// Where a new box for a source of `radius` at `center` goes, snapped to a
/// lattice of `BOX_ORIGIN_SNAP_CELLS` cells.
fn place_box(center: [f32; 3], cell: f32, below: f32) -> [f32; 3] {
    let edge = cell * LOCAL_GRID_CELLS as f32;
    let snap = cell * BOX_ORIGIN_SNAP_CELLS;
    let raw = [
        center[0] - 0.5 * edge,
        center[1] - below * edge,
        center[2] - 0.5 * edge,
    ];
    raw.map(|v| (v / snap).floor() * snap)
}

/// One transported emitter in reach this frame: what `LocalBoxSet::update`
/// matches to a box or creates a box for.
struct Source {
    center: [f32; 3],
    /// What the box has to contain: the source radius, scaled up for an
    /// explosion to where its gas goes.
    radius: f32,
    wanted: Option<f32>,
    below: f32,
    is_explosion: bool,
    distance_sq: f32,
}

/// Host-side bookkeeping of which slots of the atlas hold which box.
///
/// Persistent across frames so a box keeps its field; a source that no box
/// holds gets a free slot, and a box with no source for the linger window
/// is released.
#[derive(Default)]
pub(crate) struct LocalBoxSet {
    boxes: [Option<LocalBox>; MAX_LOCAL_BOXES],
    /// `update`'s per-frame source list, kept between frames so a scene with a
    /// fire in reach does not allocate one every frame (#6753). Only its
    /// capacity persists: `update` clears it before use.
    sources: Vec<Source>,
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
    /// `reach_world` bounds how far from the camera a NEW box is created (a
    /// source beyond it is left to the froxels); it is also capped by the
    /// camera-centred cluster cube the shader's source lookup
    /// (`localFogCluster`) is limited to, which the caller folds in. A source
    /// already held by a box is kept out to `LOCAL_GRID_REACH_HYSTERESIS` times
    /// the reach.
    pub(crate) fn update(
        &mut self,
        volumes: &[GpuFogVolume],
        camera: [f32; 3],
        reach_world: f32,
        now: f32,
        linger_seconds: f32,
    ) {
        // Taken out of `self` so the loop below can borrow `self.boxes`
        // mutably; handed back (capacity kept) after the last use.
        let mut sources = std::mem::take(&mut self.sources);
        sources.clear();
        sources.extend(
            volumes
            .iter()
            .filter(|volume| is_transport_emitter(volume))
            .filter_map(|volume| {
                let center = [
                    volume.center_shape[0],
                    volume.center_shape[1],
                    volume.center_shape[2],
                ];
                let source_radius = volume.half_extents_extinction[..3]
                    .iter()
                    .fold(0.0f32, |acc, v| acc.max(*v));
                let explosion = is_explosion(volume);
                // An explosion's box is sized for its gas, and must contain that
                // whole extent (`radius`), not just the authored source.
                let (radius, wanted, below) = if explosion {
                    let radius = source_radius * BOX_EDGE_PER_EXPLOSION_RADIUS / BOX_EDGE_PER_SOURCE_RADIUS;
                    (radius, cell_for_edge(BOX_EDGE_PER_EXPLOSION_RADIUS * source_radius), BOX_BELOW_EXPLOSION)
                } else {
                    (source_radius, cell_for_source_radius(source_radius), BOX_BELOW_SOURCE)
                };
                let distance_sq: f32 = (0..3).map(|a| (center[a] - camera[a]).powi(2)).sum();
                (center.iter().all(|v| v.is_finite())
                    && radius.is_finite()
                    && radius > 0.0
                    && distance_sq.sqrt() <= reach_world * LOCAL_GRID_REACH_HYSTERESIS)
                    .then_some(Source {
                        center,
                        radius,
                        wanted,
                        below,
                        is_explosion: explosion,
                        distance_sq,
                    })
            }),
        );
        if local_grid_debug_bits() & DEBUG_ONLY_EXPLOSIONS != 0 {
            sources.retain(|source| source.is_explosion);
        }
        // Explosions first: their boxes evict overlapping finer ones before
        // those get a chance to be re-created or held this frame.
        sources.sort_by(|a, b| {
            b.is_explosion
                .cmp(&a.is_explosion)
                .then(a.distance_sq.total_cmp(&b.distance_sq))
        });

        for source in &sources {
            let Some(wanted) = source.wanted else {
                continue;
            };
            if let Some(held) = self.boxes.iter_mut().flatten().find(|b| {
                (b.explosion || b.cell <= wanted * 1.0001)
                    && b.contains_source(source.center, source.radius)
            }) {
                // A blast's box lives as long as the blast, not as long as a
                // fire it happens to cover: that one is drawn there meanwhile
                // and gets its own fine box back when the blast's box is gone.
                if !held.explosion || source.is_explosion {
                    held.last_source_time = now;
                }
                continue;
            }
            if source.distance_sq.sqrt() > reach_world {
                continue; // inside the hysteresis band: held boxes only
            }
            let origin = place_box(source.center, wanted, source.below);
            let candidate = LocalBox {
                origin,
                cell: wanted,
                last_source_time: now,
                history: false,
                explosion: source.is_explosion,
            };
            if source.is_explosion {
                // A blast's box takes over the space of any finer box it overlaps.
                for (index, slot) in self.boxes.iter_mut().enumerate() {
                    if slot.is_some_and(|b| !b.explosion && b.overlaps(&candidate)) {
                        log::debug!("local grid: box {index} evicted by an explosion box");
                        *slot = None;
                    }
                }
            } else if self
                .boxes
                .iter()
                .flatten()
                .any(|b| b.explosion && b.overlaps(&candidate))
            {
                continue; // inside a blast's box: drawn there, at its cell size
            }
            if let Some(slot) = self.boxes.iter().position(Option::is_none) {
                log::debug!(
                    "local grid: box {slot} created at t={now:.2}s, cell {:.1} cm, edge {:.2} m, \
                     source radius {:.2} m at {:.1} m",
                    wanted / WORLD_UNITS_PER_METER * 100.0,
                    wanted * LOCAL_GRID_CELLS as f32 / WORLD_UNITS_PER_METER,
                    source.radius / WORLD_UNITS_PER_METER,
                    source.distance_sq.sqrt() / WORLD_UNITS_PER_METER,
                );
                self.boxes[slot] = Some(candidate);
            }
        }

        for (index, slot) in self.boxes.iter_mut().enumerate() {
            if slot.is_some_and(|b| now - b.last_source_time > linger_seconds) {
                log::debug!("local grid: box {index} released at t={now:.2}s");
                *slot = None;
            }
        }
        sources.clear();
        self.sources = sources;
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

/// Host-only debug bit: boxes for explosions only (no flame or smoke boxes).
const DEBUG_ONLY_EXPLOSIONS: u32 = 1 << 8;
/// Host-only debug bit: no light derived from the combustion field is appended
/// to the frame's light list (the internal-glow A/B, #6535).
const DEBUG_NO_DERIVED_LIGHTS: u32 = 1 << 9;

/// Whether `BYRO_LOCAL_GRID_DEBUG=no_derived_lights` is set.
pub(crate) fn derived_lights_disabled() -> bool {
    local_grid_debug_bits() & DEBUG_NO_DERIVED_LIGHTS != 0
}

/// `BYRO_LOCAL_GRID_DEBUG`: comma-separated `no_march` (composite skips the
/// close-range march), `no_handoff` (the froxel pass keeps simulating the
/// fire inside a box) and/or `only_explosions` (no flame or smoke boxes, to look
/// at a blast without a nested box) and/or `dense_march` (composite takes eight
/// samples per cell: the converged reference for the stepping error) and/or
/// `no_species` (every cell takes the soot phase, the steam A/B baseline) and/or
/// `no_derived_lights` (no light derived from the combustion field is added to
/// the frame, the internal-glow A/B). A diagnostic for telling a fault in the local grid from
/// one in the froxels around it; unset in normal use. Read once per process.
fn local_grid_debug_bits() -> u32 {
    static BITS: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *BITS.get_or_init(|| {
        let Ok(value) = std::env::var("BYRO_LOCAL_GRID_DEBUG") else {
            return 0;
        };
        value.split(',').fold(0, |bits, word| match word.trim() {
            "no_march" => bits | LOCAL_GRID_DEBUG_NO_MARCH,
            "no_handoff" => bits | LOCAL_GRID_DEBUG_NO_HANDOFF,
            "only_explosions" => bits | DEBUG_ONLY_EXPLOSIONS,
            "no_derived_lights" => bits | DEBUG_NO_DERIVED_LIGHTS,
            "dense_march" => bits | LOCAL_GRID_DEBUG_DENSE_MARCH,
            "no_species" => bits | LOCAL_GRID_DEBUG_NO_SPECIES,
            "" => bits,
            other => {
                log::warn!("BYRO_LOCAL_GRID_DEBUG: unknown word `{other}` (use no_march, no_handoff, only_explosions, dense_march, no_species, no_derived_lights)");
                bits
            }
        })
    })
}

/// The per-frame table `include/local_grid_params.glsl` declares as the
/// `LocalGridParams` UBO, bound by the froxel inject pass (set 0, binding 26)
/// and by composite (binding 12). std140: every member is a `vec4`/`uvec4`, so
/// the Rust layout is the GLSL one with no padding.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct GpuLocalGridParams {
    /// xyz = absolute minimum corner, w = cell edge (world units), per slot.
    pub(crate) boxes: [[f32; 4]; MAX_LOCAL_BOXES],
    /// x = atlas slabs, y = history mask, z = live mask (0 disables every
    /// consumer for this frame), w = the `LOCAL_GRID_DEBUG_*` bits.
    pub(crate) info: [u32; 4],
    /// xyz = direction TO the sun.
    pub(crate) sun_dir: [f32; 4],
    /// rgb = sun radiance.
    pub(crate) sun_radiance: [f32; 4],
    /// x = forward g, y = backward g, z = forward mix.
    pub(crate) phase: [f32; 4],
    /// xyz = render origin.
    pub(crate) render_origin: [f32; 4],
}

// SAFETY: `#[repr(C)]`, every field a multiple of 16 bytes — no padding (#3761).
unsafe impl crate::vulkan::buffer::NoUninit for GpuLocalGridParams {}

impl GpuLocalGridParams {
    /// Whether the march this frame has any sun radiance to scatter: the same
    /// test composite's shader applies to `sun_radiance`, so the CPU's choice of
    /// specialised pipeline always agrees with it.
    pub(crate) fn sunlit(&self) -> bool {
        self.sun_radiance[..3].iter().any(|&c| c > 0.0)
    }

    /// The table for this frame: live boxes (when `simulated`) plus the sun and
    /// render origin the close-range march needs.
    pub(crate) fn new(boxes: &LocalBoxSet, simulated: bool, frame: &VolumetricsParams) -> Self {
        let mut params = Self::default();
        if simulated {
            for (slot, b) in boxes.boxes().iter().enumerate() {
                if let Some(b) = b {
                    params.boxes[slot] = [b.origin[0], b.origin[1], b.origin[2], b.cell];
                }
            }
            params.info = [
                boxes.slab_count() as u32,
                boxes.history_mask(),
                boxes.live_mask(),
                local_grid_debug_bits()
                    & (LOCAL_GRID_DEBUG_NO_MARCH
                        | LOCAL_GRID_DEBUG_NO_HANDOFF
                        | LOCAL_GRID_DEBUG_DENSE_MARCH
                        | LOCAL_GRID_DEBUG_NO_SPECIES),
            ];
        }
        params.sun_dir = [frame.sun_dir[0], frame.sun_dir[1], frame.sun_dir[2], 0.0];
        params.sun_radiance = [frame.sun_color[0], frame.sun_color[1], frame.sun_color[2], 0.0];
        params.phase = [frame.sun_dir[3], frame.medium_params[1], frame.medium_params[2], 0.0];
        params.render_origin = [
            frame.render_origin[0],
            frame.render_origin[1],
            frame.render_origin[2],
            0.0,
        ];
        params
    }
}

/// Everything composite binds to draw the close-range fire (set 0, bindings
/// 12..=17), per frame in flight. Borrowed from the volumetrics pipeline, which
/// owns the resources; composite writes descriptors from the handles.
#[derive(Clone, Default)]
pub struct LocalFireViews {
    /// Binding 12 — `LocalGridParams` UBO.
    pub params: Vec<vk::Buffer>,
    /// Binding 13 — this frame's state atlas.
    pub state: Vec<vk::ImageView>,
    /// Binding 14 — this frame's optical atlas.
    pub optical: Vec<vk::ImageView>,
    /// Binding 15 — the irradiance field the inject pass writes.
    pub irradiance: Vec<vk::ImageView>,
    /// Binding 16 — occupancy bricks (`BRICK_BUFFER_BYTES`).
    pub bricks: Vec<vk::Buffer>,
    /// Binding 17 — the baked species phase table (one image, not per frame).
    pub phase_table: vk::ImageView,
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
        // 7: occupancy bricks, written by the sim, read by composite's march.
        compute_binding(7, vk::DescriptorType::STORAGE_BUFFER),
        // 8: the species phase table, written once by `combustion_phase_bake.comp`
        // (the sim does not touch it; the bake shares this layout).
        compute_binding(8, vk::DescriptorType::STORAGE_IMAGE),
    ]
}

/// Reflect set 1 of the local grid's two compute shaders (`volumetrics_local_sim.comp`
/// and `combustion_phase_bake.comp`, which share one layout) against
/// [`local_grid_layout_bindings`]. Needs no device: `LocalGrid::build` runs it
/// before creating the layout, and `reflect.rs` runs it in the static layout
/// test, so a drift fails `cargo test` instead of engine start (#6752).
pub(crate) fn validate_local_grid_layout() -> Result<()> {
    validate_set_layout(
        1,
        &local_grid_layout_bindings(),
        &[
            ReflectedShader {
                name: "volumetrics_local_sim.comp",
                spirv: VOLUMETRICS_LOCAL_SIM_COMP_SPV,
            },
            ReflectedShader {
                name: "combustion_phase_bake.comp",
                spirv: COMBUSTION_PHASE_BAKE_COMP_SPV,
            },
        ],
        "volumetrics_local_grid",
        &[],
    )
}

/// Bindings 26..=31 added to the froxel inject pass's set 0 for the local grid
/// (the declarations are in `include/volumetrics_bindings.glsl`).
pub(crate) fn inject_local_grid_bindings() -> Vec<vk::DescriptorSetLayoutBinding<'static>> {
    vec![
        compute_binding(26, vk::DescriptorType::UNIFORM_BUFFER),
        compute_binding(27, vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
        compute_binding(28, vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
        compute_binding(29, vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
        compute_binding(30, vk::DescriptorType::STORAGE_IMAGE),
        compute_binding(31, vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
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

/// The irradiance image's extent for a froxel grid: radiance and mean light
/// direction side by side, so twice the grid's width
/// (`include/local_irradiance.glsl`).
pub(crate) fn irradiance_extent(froxel: vk::Extent3D) -> vk::Extent3D {
    vk::Extent3D {
        width: froxel.width * 2,
        ..froxel
    }
}

/// Bytes of the froxel-resolution irradiance field (RGBA16F), all slots.
pub(crate) fn irradiance_resident_bytes(extent: vk::Extent3D) -> u64 {
    let extent = irradiance_extent(extent);
    u64::from(extent.width) * u64::from(extent.height) * u64::from(extent.depth) * 8
        * MAX_FRAMES_IN_FLIGHT as u64
}

pub(crate) struct LocalGrid {
    froxel_extent: vk::Extent3D,
    /// Per-frame `LocalGridParams` UBO (host-visible).
    params_buffers: Vec<GpuBuffer>,
    /// Whether the last published table carried any sun radiance; composite
    /// picks its sun or sun-less pipeline from it (see `GpuLocalGridParams::sunlit`).
    sunlit: bool,
    /// Per-frame occupancy bricks (device-local; cleared, then written by the sim).
    brick_buffers: Vec<GpuBuffer>,
    /// Froxel-resolution irradiance field, written by the inject pass, read
    /// by the next inject (temporal) and by composite's fire march.
    irradiance: Vec<GpuImage>,
    set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    /// Fills `phase_table` once, from `initialize_layouts`.
    phase_bake_pipeline: vk::Pipeline,
    /// The species octave lobes over the scattering angle (rows: soot, steam),
    /// composite's binding 17. Baked from `include/combustion_phase.glsl`.
    phase_table: Option<GpuImage>,
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
        froxel_extent: vk::Extent3D,
    ) -> Result<Self> {
        let mut grid = Self {
            froxel_extent,
            params_buffers: Vec::new(),
            sunlit: true,
            brick_buffers: Vec::new(),
            irradiance: Vec::new(),
            set_layout: vk::DescriptorSetLayout::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            phase_bake_pipeline: vk::Pipeline::null(),
            phase_table: None,
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
        validate_local_grid_layout()
            .context("local grid layout drifted against volumetrics_local_sim.comp")?;
        let bindings = local_grid_layout_bindings();

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
        self.phase_bake_pipeline = super::super::pipeline::create_compute_pipeline(
            device,
            pipeline_cache,
            COMBUSTION_PHASE_BAKE_COMP_SPV,
            self.pipeline_layout,
            "Combustion phase table bake",
        )?;

        // SAFETY: `device` is live; handle owned by `self`.
        self.sampler =
            unsafe { clamp_sampler(device, vk::Filter::LINEAR, vk::SamplerMipmapMode::NEAREST) }
                .context("local grid sampler")?;

        let usage = vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_DST;
        self.phase_table = Some(GpuImage::create(
            device,
            allocator,
            &GpuImageDesc::color_2d(
                "combustion_phase_table",
                COMBUSTION_PHASE_TABLE_WIDTH,
                PHASE_TABLE_ROWS,
                COMBUSTION_FIELD_FORMAT,
                usage,
            ),
        )?);
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
            let mut params = GpuBuffer::create_host_visible(
                device,
                allocator,
                std::mem::size_of::<GpuLocalGridParams>() as vk::DeviceSize,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            )?;
            // A zero table (live mask 0) until the first dispatch writes one.
            let zeroed = params.write_mapped(device, std::slice::from_ref(&GpuLocalGridParams::default()));
            self.params_buffers.push(params);
            zeroed?;
            self.brick_buffers.push(GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                BRICK_BUFFER_BYTES,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            )?);
            self.irradiance.push(GpuImage::create(
                device,
                allocator,
                &GpuImageDesc::color_3d(
                    &format!("volumetrics_local_irradiance_{i}"),
                    irradiance_extent(self.froxel_extent),
                    FROXEL_FORMAT,
                    usage,
                ),
            )?);
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
            let bricks_info = [vk::DescriptorBufferInfo {
                buffer: self.brick_buffers[f].buffer,
                offset: 0,
                range: BRICK_BUFFER_BYTES,
            }];
            let phase_table = [storage_image_info(
                self.phase_table.as_ref().expect("phase table").view,
            )];
            let set = self.sets[f];
            let writes = [
                write_storage_image(set, 0, &write_state),
                write_storage_image(set, 1, &write_dynamics),
                write_storage_image(set, 2, &write_optical),
                write_combined_image_sampler(set, 3, &read_state),
                write_combined_image_sampler(set, 4, &read_dynamics),
                write_combined_image_sampler(set, 5, &read_optical),
                write_storage_buffer(set, 6, &stats_info),
                write_storage_buffer(set, 7, &bricks_info),
                write_storage_image(set, 8, &phase_table),
            ];
            // SAFETY: the sets are freshly allocated and not yet in use.
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }
        log::info!(
            "Volumetrics local grid created: {} boxes of {}^3 cells, {} MiB atlas (3 x RGBA16F x {} slots) + {} MiB irradiance field",
            MAX_LOCAL_BOXES,
            LOCAL_GRID_CELLS,
            local_grid_resident_bytes() / (1024 * 1024),
            MAX_FRAMES_IN_FLIGHT,
            irradiance_resident_bytes(self.froxel_extent) / (1024 * 1024),
        );
        Ok(())
    }

    /// Every atlas image, for the one-time layout transition and clear.
    pub(crate) fn images(&self) -> impl Iterator<Item = &GpuImage> {
        self.state
            .iter()
            .chain(self.dynamics.iter())
            .chain(self.optical.iter())
            .chain(self.irradiance.iter())
            .chain(self.phase_table.iter())
    }

    /// Fill the phase table. Records into the one-time initialisation command
    /// buffer, after the images were moved to GENERAL and cleared (the caller
    /// has made the clear visible to compute); the submit's fence publishes the
    /// result to composite.
    ///
    /// # Safety
    ///
    /// `cmd` must be recording on `device`, and the table must be in GENERAL.
    pub(crate) unsafe fn record_phase_bake(&self, device: &ash::Device, cmd: vk::CommandBuffer) {
        // SAFETY: caller contract; the pipeline, layout and set outlive the
        // submit. Only set 1 is bound: the bake shader declares nothing in set 0.
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.phase_bake_pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline_layout,
                1,
                &self.sets[..1],
                &[],
            );
            device.cmd_dispatch(
                cmd,
                COMBUSTION_PHASE_TABLE_WIDTH.div_ceil(PHASE_BAKE_WORKGROUP),
                PHASE_TABLE_ROWS,
                1,
            );
        }
    }

    /// Descriptor handles composite needs to draw the close-range fire.
    pub(crate) fn fire_views(&self) -> LocalFireViews {
        LocalFireViews {
            params: self.params_buffers.iter().map(|b| b.buffer).collect(),
            state: self.state.iter().map(|i| i.view).collect(),
            optical: self.optical.iter().map(|i| i.view).collect(),
            irradiance: self.irradiance.iter().map(|i| i.view).collect(),
            bricks: self.brick_buffers.iter().map(|b| b.buffer).collect(),
            phase_table: self.phase_table.as_ref().expect("phase table").view,
        }
    }

    /// Write bindings 26..=31 of the froxel inject pass's descriptor set for
    /// `frame`: the table, this frame's atlas (read), and the irradiance field
    /// (this slot written, the previous one sampled).
    pub(crate) fn write_inject_bindings(
        &self,
        device: &ash::Device,
        set: vk::DescriptorSet,
        frame: usize,
    ) {
        let previous = (frame + MAX_FRAMES_IN_FLIGHT - 1) % MAX_FRAMES_IN_FLIGHT;
        let params_info = [vk::DescriptorBufferInfo {
            buffer: self.params_buffers[frame].buffer,
            offset: 0,
            range: std::mem::size_of::<GpuLocalGridParams>() as vk::DeviceSize,
        }];
        let state = [general_sampled_image_info(self.sampler, self.state[frame].view)];
        let dynamics = [general_sampled_image_info(self.sampler, self.dynamics[frame].view)];
        let optical = [general_sampled_image_info(self.sampler, self.optical[frame].view)];
        let irradiance_write = [storage_image_info(self.irradiance[frame].view)];
        let irradiance_previous =
            [general_sampled_image_info(self.sampler, self.irradiance[previous].view)];
        let writes = [
            write_uniform_buffer(set, 26, &params_info),
            write_combined_image_sampler(set, 27, &state),
            write_combined_image_sampler(set, 28, &dynamics),
            write_combined_image_sampler(set, 29, &optical),
            write_storage_image(set, 30, &irradiance_write),
            write_combined_image_sampler(set, 31, &irradiance_previous),
        ];
        // SAFETY: `set` is the inject pass's freshly written, not-yet-submitted
        // descriptor set; the referenced views belong to this grid.
        unsafe { device.update_descriptor_sets(&writes, &[]) };
    }

    /// Whether the table published for this frame has sun radiance, i.e. which
    /// composite pipeline the march needs.
    pub(crate) fn sunlit(&self) -> bool {
        self.sunlit
    }

    /// Publish this frame's table. Call after `begin_frame` has placed the
    /// boxes; `simulated` is whether the sim pass will run (a zero live mask
    /// disables the hand-off and the march for the frame).
    pub(crate) fn write_params(
        &mut self,
        device: &ash::Device,
        frame: usize,
        frame_params: &VolumetricsParams,
    ) -> Result<()> {
        let params = GpuLocalGridParams::new(&self.boxes, self.has_work(), frame_params);
        self.sunlit = params.sunlit();
        self.params_buffers[frame].write_mapped(device, std::slice::from_ref(&params))
    }

    /// Disable every consumer of this slot's table (a frame the volumetric
    /// passes did not run, so the atlas and irradiance field are stale).
    pub(crate) fn clear_params(&mut self, device: &ash::Device, frame: usize) -> Result<()> {
        // The cleared table carries no sun and disables the march outright.
        self.sunlit = false;
        self.params_buffers[frame]
            .write_mapped(device, std::slice::from_ref(&GpuLocalGridParams::default()))
    }

    /// Barriers into and out of the frame's local-grid work, and the sim itself
    /// when a box is live. Records, in order: history barriers for this slot's
    /// atlas and irradiance field; the sim dispatch (timed); a write-to-read
    /// barrier so the inject pass sees the atlas.
    ///
    /// # Safety
    ///
    /// As [`Self::record_sim`].
    pub(crate) unsafe fn record_frame(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        inject_set: vk::DescriptorSet,
        timers: Option<&mut super::super::gpu_timers::GpuPerFrameTimers>,
    ) {
        let previous = (frame + MAX_FRAMES_IN_FLIGHT - 1) % MAX_FRAMES_IN_FLIGHT;
        // The irradiance field is written by every inject dispatch, boxes or
        // not, so its history barrier is unconditional.
        let irradiance = [
            image_barrier_general_read_to_write(self.irradiance[frame].image),
            image_barrier_general_write_to_read(self.irradiance[previous].image),
        ];
        // SAFETY: caller contract — `cmd` is recording, the images live in GENERAL.
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &irradiance,
            );
        }
        if self.has_work() {
            // SAFETY: same contract.
            unsafe { self.record_sim(device, cmd, frame, inject_set, timers) };
        }
    }

    /// Publish the sim's occupancy-brick writes to composite's fragment reads.
    ///
    /// # Safety
    ///
    /// `cmd` must be recording outside a render pass.
    pub(crate) unsafe fn record_publish_bricks(&self, device: &ash::Device, cmd: vk::CommandBuffer) {
        super::memory_barrier(
            device,
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_WRITE,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::AccessFlags::SHADER_READ,
        );
    }

    /// Barriers publishing this frame's compute writes to composite's fragment
    /// reads: the atlas (state / optical) and the irradiance field.
    pub(crate) fn composite_barriers(&self, frame: usize) -> [vk::ImageMemoryBarrier<'static>; 3] {
        [
            image_barrier_general_write_to_read(self.state[frame].image),
            image_barrier_general_write_to_read(self.optical[frame].image),
            image_barrier_general_write_to_read(self.irradiance[frame].image),
        ]
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
    #[allow(clippy::too_many_arguments)] // Per-frame placement inputs, all scalars from the caller's frame state.
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
    unsafe fn record_sim(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        inject_set: vk::DescriptorSet,
        mut timers: Option<&mut super::super::gpu_timers::GpuPerFrameTimers>,
    ) {
        // Zero this slot's occupancy bricks; the sim only ever sets them.
        // SAFETY: caller contract — `cmd` is recording outside a render pass and
        // this slot's brick buffer is idle (its last reader, composite, finished
        // two frames ago and the all-slots fence wait has retired it).
        unsafe {
            device.cmd_fill_buffer(cmd, self.brick_buffers[frame].buffer, 0, BRICK_BUFFER_BYTES, 0);
        }
        super::memory_barrier(
            device,
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::AccessFlags::TRANSFER_WRITE,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
        );
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
        // The inject pass samples this frame's atlas next (hand-off).
        let published = [
            image_barrier_general_write_to_read(self.state[frame].image),
            image_barrier_general_write_to_read(self.dynamics[frame].image),
            image_barrier_general_write_to_read(self.optical[frame].image),
        ];
        // SAFETY: as above; the barrier only orders this dispatch's writes.
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &published,
            );
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
        for mut image in self.irradiance.drain(..).chain(self.phase_table.take()) {
            image.destroy(device, allocator);
        }
        for buffer in self
            .stats_buffers
            .iter_mut()
            .chain(self.params_buffers.iter_mut())
            .chain(self.brick_buffers.iter_mut())
        {
            buffer.destroy(device, allocator);
        }
        self.stats_buffers.clear();
        self.params_buffers.clear();
        self.brick_buffers.clear();
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
            if self.phase_bake_pipeline != vk::Pipeline::null() {
                device.destroy_pipeline(self.phase_bake_pipeline, None);
                self.phase_bake_pipeline = vk::Pipeline::null();
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
        let mut v = GpuFogVolume {
            center_shape: [
                center_m[0] * METER,
                center_m[1] * METER,
                center_m[2] * METER,
                0.0,
            ],
            half_extents_extinction: [radius_m * METER, radius_m * METER, radius_m * METER, 1.0],
            ..Default::default()
        };
        v.profile_params[0] = FOG_VOLUME_PROFILE_FLAME;
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
    fn a_fire_beyond_the_near_reach_is_not_given_a_box_but_a_held_one_keeps_it() {
        let reach = LOCAL_GRID_REACH_METERS * METER;
        let mut set = LocalBoxSet::default();
        let near = flame([LOCAL_GRID_REACH_METERS - 1.0, 0.5, 0.0], 0.3);
        let just_out = flame([LOCAL_GRID_REACH_METERS + 2.0, 0.5, 0.0], 0.3);
        set.update(&[just_out], [0.0; 3], reach, 0.0, 10.0);
        assert_eq!(set.live_count(), 0, "new boxes only inside the reach");
        set.update(&[near], [0.0; 3], reach, 0.0, 10.0);
        assert_eq!(set.live_count(), 1);
        // The camera backs off: the same fire is now 2 m past the reach, inside
        // the hysteresis band, and keeps its box (and refreshes its timer).
        let held_origin = set.boxes()[0].unwrap().origin;
        set.update(&[near], [-3.0 * METER, 0.0, 0.0], reach, 5.0, 10.0);
        assert_eq!(set.live_count(), 1);
        assert_eq!(set.boxes()[0].unwrap().origin, held_origin);
        assert_eq!(set.boxes()[0].unwrap().last_source_time, 5.0);
        // Far past the band it is no longer refreshed and ages out.
        set.update(&[near], [-30.0 * METER, 0.0, 0.0], reach, 20.0, 10.0);
        assert_eq!(set.live_count(), 0);
    }

    fn explosion(center_m: [f32; 3], radius_m: f32) -> GpuFogVolume {
        let mut v = flame(center_m, radius_m);
        v.profile_params[0] = FOG_VOLUME_PROFILE_EXPLOSION_NUCLEAR;
        v
    }

    #[test]
    fn an_explosion_box_is_sized_for_the_gas_not_the_authored_source() {
        // The lab's nuclear source is 0.72 m; its dome reaches several metres.
        let mut set = LocalBoxSet::default();
        update(&mut set, &[explosion([2.0, 1.0, 3.0], 0.72)], 0.0);
        let b = set.boxes()[0].expect("an explosion gets a box");
        // 16 source radii = 11.5 m: the 20 cm cell (12.8 m edge), where a flame
        // of the same radius would have taken the 10 cm one (6.4 m).
        assert_eq!(b.cell, 0.20 * METER);
        assert_eq!(cell_for_source_radius(0.72 * METER), Some(0.10 * METER));
        // Centred vertically (a blast expands every way), not given the
        // flame's headroom.
        let centre_y = 1.0 * METER;
        let below = centre_y - b.origin[1];
        assert!(
            (below - 0.5 * b.edge()).abs() <= b.cell * BOX_ORIGIN_SNAP_CELLS,
            "centred within the origin snap, got {below} of {}",
            b.edge()
        );
        // It holds the whole expected extent.
        assert!(b.contains_source(
            [2.0 * METER, centre_y, 3.0 * METER],
            0.72 * METER * BOX_EDGE_PER_EXPLOSION_RADIUS / BOX_EDGE_PER_SOURCE_RADIUS
        ));
    }

    #[test]
    fn an_explosion_box_replaces_an_overlapping_flame_box_and_holds_its_flame() {
        let mut set = LocalBoxSet::default();
        let torch = flame([2.0, 0.5, 3.0], 0.15);
        update(&mut set, &[torch], 0.0);
        let torch_box = set.boxes()[0].unwrap();
        assert_eq!(torch_box.cell, 0.025 * METER);
        assert!(!torch_box.explosion);
        // A blast two metres away: its 12.8 m box covers the torch's.
        let blast = explosion([4.0, 1.0, 3.0], 0.72);
        update(&mut set, &[torch, blast], 1.0);
        assert_eq!(set.live_count(), 1, "one box where there were two would overlap");
        let blast_box = set.boxes().iter().flatten().next().unwrap();
        assert!(blast_box.explosion);
        assert!(blast_box.contains_source([2.0 * METER, 0.5 * METER, 3.0 * METER], 0.15 * METER));
        // The torch keeps being held by the blast's coarser box instead of
        // getting a nested fine one back.
        update(&mut set, &[torch, blast], 2.0);
        assert_eq!(set.live_count(), 1);
        // Once the blast is over and its box has lingered out, the torch gets a
        // fine box again.
        let mut now = 2.0;
        while set.live_count() > 0 && now < 60.0 {
            now += 1.0;
            update(&mut set, &[torch], now);
        }
        update(&mut set, &[torch], now + 1.0);
        let again = set.boxes().iter().flatten().next().expect("torch box returns");
        assert_eq!(again.cell, 0.025 * METER);
        assert!(!again.explosion);
    }

    #[test]
    fn local_grid_shaders_pin_the_bfecc_seam_and_the_open_boundary() {
        let sim = include_str!("../../../shaders/volumetrics_local_sim.comp");
        let inject = include_str!("../../../shaders/volumetrics_inject.comp");
        let transport = include_str!("../../../shaders/include/combustion_transport.glsl");
        // The transport takes the strength through the seam, never straight
        // from the host uniform, so the local grid can run without it.
        assert!(transport.contains("* bfeccStrengthMultiplier();"));
        assert!(!transport.contains("params.wind_gust.y"));
        assert!(sim.contains("float bfeccStrengthMultiplier() {\n    return 0.0;\n}"));
        assert!(inject.contains("float bfeccStrengthMultiplier() {\n    return params.wind_gust.y;\n}"));
        // Open boundary: outside the box reads as clear air, not "no history".
        let open = sim
            .split("greaterThanEqual(cells, vec3(dim))")
            .nth(1)
            .and_then(|rest| rest.split("vec3 uvw = cells / dim;").next())
            .expect("sampler's out-of-box branch");
        assert!(open.contains("return true;"), "out-of-box must read as open air");
        // ... unless another live box with history covers the point: a nested
        // fine box is fed by its parent, or the parent's gas is erased inside it.
        assert!(open.contains("push.info.y & (1u << b)") && open.contains("push.boxes[b].xyz"));
    }

    #[test]
    fn passive_fog_and_out_of_reach_sources_get_no_box() {
        let mut set = LocalBoxSet::default();
        let mut fog = flame([2.0, 0.5, 3.0], 0.3);
        fog.profile_params[0] = FOG_VOLUME_PROFILE_HOMOGENEOUS;
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

    /// #6749 — the ledger's irradiance and brick figures come from the code, not
    /// from a remembered number (the field is twice the froxel width, which the
    /// first version of the row missed by 2x).
    #[test]
    fn memory_budget_doc_states_the_irradiance_field_and_brick_costs() {
        const DOC: &str = include_str!("../../../../../docs/engine/memory-budget.md");
        let extent = vk::Extent3D { width: 160, height: 90, depth: 64 };
        let mib = irradiance_resident_bytes(extent) / (1024 * 1024);
        assert!(
            DOC.contains(&format!("≈ {mib} MiB at 1280×720")),
            "memory-budget.md must state the {mib} MiB irradiance field at 1280×720 \
             (`irradiance_resident_bytes`, width doubled by `irradiance_extent`)"
        );
        let kib = BRICK_BUFFER_BYTES / 1024;
        assert!(
            DOC.contains(&format!("an {kib} KiB device-local occupancy-brick buffer")),
            "memory-budget.md must state the {kib} KiB brick buffer (`BRICK_BUFFER_BYTES`)"
        );
    }

    #[test]
    fn resident_vram_is_small_enough_for_the_budget() {
        // 4 boxes of 64^3 cells, three RGBA16F fields, two slots.
        assert_eq!(local_grid_resident_bytes(), 64 * 64 * 64 * 4 * 8 * 3 * 2);
        assert!(local_grid_resident_bytes() < 64 * 1024 * 1024);
    }
}
