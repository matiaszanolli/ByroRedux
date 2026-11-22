//! TSD-4.1 terrain displacement patches, host side (#7152).
//!
//! Design: `docs/engine/terrain-displacement.md` §3. The renderer owns the
//! pool of reserved patch meshes and the compute pass that displaces them
//! (`byroredux_renderer::vulkan::terrain_patch`); this module decides every
//! frame which 256-unit squares of which resident LAND cells hold a patch, and
//! hands that to the two consumers:
//!
//! - [`emit_patch_draws`] (inside `build_render_data`, before the sort) clones
//!   each patch's base cell draw command onto the patch mesh, with
//!   `in_tlas = false` — 4.1 is raster only;
//! - [`push_terrain_patches`] (after it, with the renderer) stages the slots
//!   and the shared parameters for `draw_frame`'s compute pass, and publishes
//!   the last seam check for the `terrain.patches` console command.
//!
//! Assignment is by distance: every square whose footprint comes within
//! `range` of the camera (XZ) gets a patch, nearest first while slots last; a
//! patch keeps its slot until its square is more than `range + HYSTERESIS`
//! away. The distance fade in the shader reaches zero at `range`, so the whole
//! faded disc is covered whenever the pool is large enough — and where it is
//! not, the missing-neighbour fade brings the edge down to the lip.
//!
//! Each patch also picks its lattice level by the same distance: the fine
//! 5.7 cm lattice within `fine range` (kept until `FINE_HYSTERESIS` past it),
//! the coarse 11.4 cm one beyond — a quarter of the triangles where the
//! relief is a few pixels tall. The raster cost of the single-level pool
//! scaled with triangle density (§8.2), so this is where it goes.

use byroredux_core::ecs::{EntityId, MeshHandle, Resource, World};
use byroredux_core::lighting::BETHESDA_UNITS_PER_METER as UNITS_PER_M;
use byroredux_core::math::{Mat4, Vec3, Vec4};
use byroredux_renderer::shader_constants::{
    TERRAIN_PATCHES_PER_CELL_SIDE, TERRAIN_PATCH_LATTICE_STEP, TERRAIN_PATCH_QUADS,
    TERRAIN_PATCH_SLOTS,
};
use byroredux_renderer::vulkan::terrain_patch::{
    neighbour_bit, TerrainPatchMeshes, TerrainPatchParams, TerrainPatchSlot, TerrainPatchStats,
};
use byroredux_renderer::DrawCommand;

use crate::components::{TerrainCellOrigin, TerrainTileSlot};

const SLOTS: usize = TERRAIN_PATCH_SLOTS as usize;
const PER_SIDE: i32 = TERRAIN_PATCHES_PER_CELL_SIDE as i32;
/// World units across one patch square (256).
const PATCH_SIZE: f32 = TERRAIN_PATCH_LATTICE_STEP * TERRAIN_PATCH_QUADS as f32;

/// Defaults (§9: 3 cm amplitude, 6 m radius). Tunable through
/// `terrain.patches`; the settings/UI pass is stage 4.3.
pub(crate) const DEFAULT_AMPLITUDE_M: f32 = 0.03;
pub(crate) const DEFAULT_RANGE_M: f32 = 6.0;
/// Squares within this distance draw the fine lattice, the rest the coarse
/// one. `terrain.patches fine 0` makes every patch coarse, a fine range at or
/// past `range` every patch fine (the single-level pool).
pub(crate) const DEFAULT_FINE_RANGE_M: f32 = 2.0;
/// A fine patch stays fine until its square is this much past the fine range.
pub(crate) const FINE_HYSTERESIS_M: f32 = 0.5;
/// The 1 mm step every patch border and every undisplaced point sits at above
/// the base surface (§3.3–3.4).
pub(crate) const LIP_M: f32 = 0.001;
/// Width of the fade to the lip at a cell border or a missing neighbour (§3.4:
/// 0.5–1 m). Must stay below one patch, so the 3×3 neighbourhood is all a
/// border vertex can see.
pub(crate) const EDGE_RAMP_M: f32 = 1.0;
/// A patch keeps its slot until its square is this much farther than `range`.
pub(crate) const HYSTERESIS_M: f32 = 1.0;
/// The distance fade starts at this fraction of `range`, the form TSD-3's
/// parallax fade uses.
const FADE_START_FRACTION: f32 = 0.5;
/// Patch draws carry synthetic entity ids counting down from here, one per
/// slot, so the sort's tiebreaker stays unique (particles do the same).
const PATCH_ENTITY_ID_BASE: u32 = u32::MAX - 1;

/// One patch: the square at (`col`, `row`) of `cell`'s 16×16 grid. Rows grow
/// toward −Z, as the LAND vertex rows do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct PatchKey {
    pub(crate) cell: EntityId,
    pub(crate) col: u32,
    pub(crate) row: u32,
}

/// A resident LAND cell a patch can sit on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PatchCell {
    pub(crate) entity: EntityId,
    pub(crate) origin_xz: [f32; 2],
    pub(crate) base_mesh: u32,
    pub(crate) tile: u32,
    /// The XCLC Hide Quad bits the base mesh dropped (`TerrainCellOrigin`).
    pub(crate) hidden_quadrants: u8,
}

/// Whether patch square (`col`, `row`) lies in a quadrant the base cell
/// hides (#6043). A patch square is 2×2 LAND quads, so the quadrant split
/// falls between squares 7 and 8 on each axis; quadrant `q` is
/// `[SW, SE, NW, NE]` with rows growing north, as `land_tile_indices` reads it.
fn square_hidden(hidden_quadrants: u8, col: i32, row: i32) -> bool {
    let half = PER_SIDE / 2;
    let quadrant = u32::from(row >= half) * 2 + u32::from(col >= half);
    hidden_quadrants & (1 << quadrant) != 0
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PatchAssignment {
    pub(crate) key: PatchKey,
    pub(crate) cell: PatchCell,
    pub(crate) missing_mask: u32,
    /// Draws the coarse lattice.
    pub(crate) coarse: bool,
    /// Resident neighbours drawing the coarse lattice (`missing_mask` bits).
    pub(crate) coarse_mask: u32,
}

/// The distances `assign_patches` works to, world units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PatchRanges {
    pub(crate) range: f32,
    pub(crate) hysteresis: f32,
    pub(crate) fine_range: f32,
    pub(crate) fine_hysteresis: f32,
}

/// The pool's host-side state.
pub(crate) struct TerrainPatchState {
    /// `terrain.patches off` turns the whole stage off (the A/B switch).
    pub(crate) enabled: bool,
    pub(crate) amplitude_m: f32,
    pub(crate) range_m: f32,
    pub(crate) fine_range_m: f32,
    /// Keep the TSD-3 parallax march on patch draws (default on; see
    /// `TerrainPatchParams::parallax`).
    pub(crate) parallax: bool,
    /// The renderer's reserved patch meshes, indexed by slot; copied on the
    /// first frame with a renderer. Empty means no patches.
    pub(crate) meshes: TerrainPatchMeshes,
    pub(crate) slots: [Option<PatchAssignment>; SLOTS],
    /// The camera position the slots were assigned from, world units.
    pub(crate) camera_pos: [f32; 3],
    /// The renderer's last seam check, for `terrain.patches`.
    pub(crate) stats: TerrainPatchStats,
}

impl Default for TerrainPatchState {
    fn default() -> Self {
        Self {
            enabled: true,
            amplitude_m: DEFAULT_AMPLITUDE_M,
            range_m: DEFAULT_RANGE_M,
            fine_range_m: DEFAULT_FINE_RANGE_M,
            parallax: true,
            meshes: TerrainPatchMeshes::default(),
            slots: [None; SLOTS],
            camera_pos: [0.0; 3],
            stats: TerrainPatchStats::default(),
        }
    }
}

impl Resource for TerrainPatchState {}

impl TerrainPatchState {
    pub(crate) fn resident(&self) -> usize {
        self.slots.iter().flatten().count()
    }

    pub(crate) fn ranges(&self) -> PatchRanges {
        PatchRanges {
            range: self.range_m * UNITS_PER_M,
            hysteresis: HYSTERESIS_M * UNITS_PER_M,
            fine_range: self.fine_range_m * UNITS_PER_M,
            fine_hysteresis: FINE_HYSTERESIS_M * UNITS_PER_M,
        }
    }

    /// The shader parameters, in world units.
    pub(crate) fn params(&self) -> TerrainPatchParams {
        let range = self.range_m * UNITS_PER_M;
        TerrainPatchParams {
            camera_pos: self.camera_pos,
            amplitude: self.amplitude_m * UNITS_PER_M,
            lip: LIP_M * UNITS_PER_M,
            fade_start: FADE_START_FRACTION * range,
            fade_end: range,
            edge_ramp: EDGE_RAMP_M * UNITS_PER_M,
            parallax: self.parallax,
        }
    }
}

/// Distance in XZ from cell-local point (`lx`, `lz`) — `lx` along +X from the
/// cell origin, `lz` along −Z — to the square at (`col`, `row`).
fn distance_to_square(lx: f32, lz: f32, col: i32, row: i32) -> f32 {
    let (x0, z0) = (col as f32 * PATCH_SIZE, row as f32 * PATCH_SIZE);
    let dx = (x0 - lx).max(lx - (x0 + PATCH_SIZE)).max(0.0);
    let dz = (z0 - lz).max(lz - (z0 + PATCH_SIZE)).max(0.0);
    (dx * dx + dz * dz).sqrt()
}

/// Reassign `slots` for a camera at `cam_xz` (world units), keeping every
/// patch that is still wanted in the slot it already has, and pick each
/// patch's lattice level.
pub(crate) fn assign_patches(
    slots: &mut [Option<PatchAssignment>; SLOTS],
    cells: &[PatchCell],
    cam_xz: [f32; 2],
    ranges: PatchRanges,
) {
    let PatchRanges { range, hysteresis, fine_range, fine_hysteresis } = ranges;
    let keep_range = range + hysteresis;
    let mut candidates: Vec<(f32, PatchKey, PatchCell)> = Vec::new();
    for cell in cells {
        let lx = cam_xz[0] - cell.origin_xz[0];
        let lz = cell.origin_xz[1] - cam_xz[1];
        let lo_col = (((lx - keep_range) / PATCH_SIZE).floor() as i32).max(0);
        let hi_col = (((lx + keep_range) / PATCH_SIZE).floor() as i32).min(PER_SIDE - 1);
        let lo_row = (((lz - keep_range) / PATCH_SIZE).floor() as i32).max(0);
        let hi_row = (((lz + keep_range) / PATCH_SIZE).floor() as i32).min(PER_SIDE - 1);
        for row in lo_row..=hi_row {
            for col in lo_col..=hi_col {
                // The base draws nothing there, so neither does a patch; the
                // square counts as missing and its neighbours fade to the lip.
                if square_hidden(cell.hidden_quadrants, col, row) {
                    continue;
                }
                let dist = distance_to_square(lx, lz, col, row);
                let key = PatchKey { cell: cell.entity, col: col as u32, row: row as u32 };
                let held = slots.iter().flatten().any(|a| a.key == key);
                if dist <= range || (held && dist <= keep_range) {
                    candidates.push((dist, key, *cell));
                }
            }
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    candidates.truncate(SLOTS);

    // Free the slots whose patch is no longer chosen, refresh the kept ones'
    // cell record, then fill free slots in distance order.
    for slot in slots.iter_mut() {
        if let Some(assignment) = slot {
            match candidates.iter().find(|c| c.1 == assignment.key) {
                Some(&(_, _, cell)) => assignment.cell = cell,
                None => *slot = None,
            }
        }
    }
    for &(_, key, cell) in &candidates {
        if slots.iter().flatten().any(|a| a.key == key) {
            continue;
        }
        if let Some(free) = slots.iter_mut().find(|s| s.is_none()) {
            *free = Some(PatchAssignment {
                key,
                cell,
                missing_mask: 0,
                coarse: true,
                coarse_mask: 0,
            });
        }
    }

    // Level: fine within the fine range, and a fine patch stays fine until
    // the fine hysteresis past it (a new patch starts coarse).
    for assignment in slots.iter_mut().flatten() {
        let lx = cam_xz[0] - assignment.cell.origin_xz[0];
        let lz = assignment.cell.origin_xz[1] - cam_xz[1];
        let dist =
            distance_to_square(lx, lz, assignment.key.col as i32, assignment.key.row as i32);
        let fine = fine_range > 0.0
            && (dist <= fine_range
                || (!assignment.coarse && dist <= fine_range + fine_hysteresis));
        assignment.coarse = !fine;
    }

    // A neighbour square is missing unless a resident patch of the same cell
    // holds it; squares across the cell border always are (the cell-border
    // rule, §3.4).
    let resident: Vec<(PatchKey, bool)> =
        slots.iter().flatten().map(|a| (a.key, a.coarse)).collect();
    for assignment in slots.iter_mut().flatten() {
        let PatchKey { cell, col, row } = assignment.key;
        let mut mask = 0;
        let mut coarse_mask = 0;
        for dr in -1..=1 {
            for dc in -1..=1 {
                if dr == 0 && dc == 0 {
                    continue;
                }
                let (c, r) = (col as i32 + dc, row as i32 + dr);
                let inside = (0..PER_SIDE).contains(&c) && (0..PER_SIDE).contains(&r);
                let key = PatchKey { cell, col: c as u32, row: r as u32 };
                match resident.iter().find(|(k, _)| inside && *k == key) {
                    None => mask |= neighbour_bit(dr, dc),
                    Some(&(_, true)) => coarse_mask |= neighbour_bit(dr, dc),
                    Some(&(_, false)) => {}
                }
            }
        }
        assignment.missing_mask = mask;
        assignment.coarse_mask = coarse_mask;
    }
}

/// Every resident full-detail LAND cell within reach of `cam_xz`.
fn nearby_cells(world: &World, cam_xz: [f32; 2], reach: f32) -> Vec<PatchCell> {
    let (Some(origins), Some(tiles), Some(meshes)) = (
        world.query::<TerrainCellOrigin>(),
        world.query::<TerrainTileSlot>(),
        world.query::<MeshHandle>(),
    ) else {
        return Vec::new();
    };
    let cell_size = PATCH_SIZE * PER_SIDE as f32;
    let mut cells = Vec::new();
    for (entity, origin) in origins.iter() {
        let (Some(tile), Some(mesh)) = (tiles.get(entity), meshes.get(entity)) else {
            continue;
        };
        let lx = cam_xz[0] - origin.origin_xz[0];
        let lz = origin.origin_xz[1] - cam_xz[1];
        let dx = (-lx).max(lx - cell_size).max(0.0);
        let dz = (-lz).max(lz - cell_size).max(0.0);
        if dx * dx + dz * dz <= reach * reach {
            cells.push(PatchCell {
                entity,
                origin_xz: origin.origin_xz,
                base_mesh: mesh.0,
                tile: tile.0,
                hidden_quadrants: origin.hidden_quadrants,
            });
        }
    }
    cells.sort_by_key(|c| c.entity);
    cells
}

/// Assign this frame's patches and append their draws: each patch draws as a
/// copy of its base cell's command with the patch mesh swapped in, kept out of
/// the TLAS (stage 4.1 is raster only). Called before the draw sort; the patch
/// meshes are the registry's first handles, so in the opaque sort (grouped by
/// mesh) every patch draws before any base cell and early-Z rejects the base
/// under it.
pub(crate) fn emit_patch_draws(
    world: &World,
    cam_pos: Vec3,
    vp_mat: Mat4,
    draw_commands: &mut Vec<DrawCommand>,
) {
    let Some(mut state) = world.try_resource_mut::<TerrainPatchState>() else {
        return;
    };
    let state = &mut *state;
    state.camera_pos = cam_pos.to_array();
    if !state.enabled || state.meshes.fine.len() < SLOTS || state.meshes.coarse.len() < SLOTS {
        state.slots = [None; SLOTS];
        return;
    }
    let ranges = state.ranges();
    let cam_xz = [cam_pos.x, cam_pos.z];
    let cells = nearby_cells(world, cam_xz, ranges.range + ranges.hysteresis);
    assign_patches(&mut state.slots, &cells, cam_xz, ranges);

    let base_count = draw_commands.len();
    for (slot, assignment) in state.slots.iter().enumerate() {
        let Some(assignment) = assignment else {
            continue;
        };
        let Some(base) = draw_commands[..base_count].iter().find(|cmd| {
            cmd.entity_id == assignment.key.cell && cmd.terrain_tile_index.is_some()
        }) else {
            continue;
        };
        if !base.in_raster {
            continue;
        }
        let Some(mesh) = state.meshes.get(slot as u32, assignment.coarse) else {
            continue;
        };
        let mut patch = base.clone();
        patch.mesh_handle = mesh;
        patch.in_tlas = false;
        patch.entity_id = PATCH_ENTITY_ID_BASE - slot as u32;
        let centre = Vec4::new(
            assignment.cell.origin_xz[0] + (assignment.key.col as f32 + 0.5) * PATCH_SIZE,
            cam_pos.y,
            assignment.cell.origin_xz[1] - (assignment.key.row as f32 + 0.5) * PATCH_SIZE,
            1.0,
        );
        patch.sort_depth = (vp_mat * centre).w.max(0.0).to_bits();
        draw_commands.push(patch);
    }
}

/// Stage this frame's patches for the renderer's compute pass, and pick up the
/// pool and the last seam check from it. Called after `build_render_data`.
pub(crate) fn push_terrain_patches(world: &World, ctx: &mut byroredux_renderer::VulkanContext) {
    let Some(mut state) = world.try_resource_mut::<TerrainPatchState>() else {
        return;
    };
    if state.meshes.is_empty() {
        if let Some(meshes) = ctx.terrain_patch_meshes() {
            state.meshes = meshes.clone();
        }
    }
    if let Some(stats) = ctx.terrain_patch_stats() {
        state.stats = stats.clone();
    }
    let slots: Vec<TerrainPatchSlot> = state
        .slots
        .iter()
        .enumerate()
        .filter_map(|(slot, a)| {
            a.map(|a| TerrainPatchSlot {
                slot: slot as u32,
                patch_col: a.key.col,
                patch_row: a.key.row,
                cell_origin_xz: a.cell.origin_xz,
                base_mesh: a.cell.base_mesh,
                terrain_tile: a.cell.tile,
                missing_mask: a.missing_mask,
                coarse: a.coarse,
                coarse_mask: a.coarse_mask,
            })
        })
        .collect();
    if slots.is_empty() {
        return;
    }
    ctx.set_terrain_patch_frame(&slots, state.params());
}

#[cfg(test)]
mod tests {
    use super::*;

    const CELL: f32 = PATCH_SIZE * PER_SIDE as f32;

    fn cell(entity: EntityId, gx: i32, gy: i32) -> PatchCell {
        // Y-up origin of the (row 0, col 0) vertex: x = gx·cell, z = −gy·cell.
        PatchCell {
            entity,
            origin_xz: [gx as f32 * CELL, -(gy as f32) * CELL],
            base_mesh: 100 + entity,
            tile: entity,
            hidden_quadrants: 0,
        }
    }

    /// World XZ of a cell-local point (`lx` east, `lz` toward −Z).
    fn at(c: &PatchCell, lx: f32, lz: f32) -> [f32; 2] {
        [c.origin_xz[0] + lx, c.origin_xz[1] - lz]
    }

    fn keys(slots: &[Option<PatchAssignment>; SLOTS]) -> Vec<PatchKey> {
        let mut k: Vec<PatchKey> = slots.iter().flatten().map(|a| a.key).collect();
        k.sort();
        k
    }

    const RANGE: f32 = DEFAULT_RANGE_M * UNITS_PER_M;
    const HYST: f32 = HYSTERESIS_M * UNITS_PER_M;

    fn ranges(range: f32) -> PatchRanges {
        PatchRanges {
            range,
            hysteresis: HYST,
            fine_range: DEFAULT_FINE_RANGE_M * UNITS_PER_M,
            fine_hysteresis: FINE_HYSTERESIS_M * UNITS_PER_M,
        }
    }

    #[test]
    fn every_square_within_range_of_the_camera_gets_a_patch() {
        let c = cell(1, 0, 0);
        let cam = at(&c, 8.0 * PATCH_SIZE, 8.0 * PATCH_SIZE); // a patch corner, mid-cell
        let mut slots = [None; SLOTS];
        assign_patches(&mut slots, &[c], cam, ranges(RANGE));
        for row in 0..PER_SIDE {
            for col in 0..PER_SIDE {
                let d = distance_to_square(8.0 * PATCH_SIZE, 8.0 * PATCH_SIZE, col, row);
                let held = keys(&slots)
                    .contains(&PatchKey { cell: 1, col: col as u32, row: row as u32 });
                assert_eq!(held, d <= RANGE, "square ({col},{row}) at {d}");
            }
        }
        // 6 m around a corner touches the 4×4 block of squares around it.
        assert_eq!(slots.iter().flatten().count(), 16);
    }

    #[test]
    fn a_patch_keeps_its_slot_while_the_camera_moves() {
        let c = cell(1, 0, 0);
        let mut slots = [None; SLOTS];
        assign_patches(&mut slots, &[c], at(&c, 2000.0, 2000.0), ranges(RANGE));
        let before = slots;
        assign_patches(&mut slots, &[c], at(&c, 2040.0, 2010.0), ranges(RANGE));
        for (b, a) in before.iter().zip(slots.iter()) {
            if let (Some(b), Some(a)) = (b, a) {
                assert_eq!(a.key, b.key, "a patch moved slot");
            }
        }
    }

    #[test]
    fn a_patch_just_past_the_range_is_kept_by_the_hysteresis_but_not_added() {
        let c = cell(1, 0, 0);
        // Camera 10 units short of the range from square (10, 8)'s west edge.
        let lx = 10.0 * PATCH_SIZE - RANGE + 10.0;
        let target = PatchKey { cell: 1, col: 10, row: 8 };
        let mut slots = [None; SLOTS];
        assign_patches(&mut slots, &[c], at(&c, lx, 8.5 * PATCH_SIZE), ranges(RANGE));
        assert!(keys(&slots).contains(&target));
        // Back off 30 units: past the range, inside the hysteresis — kept.
        assign_patches(&mut slots, &[c], at(&c, lx - 30.0, 8.5 * PATCH_SIZE), ranges(RANGE));
        assert!(keys(&slots).contains(&target));
        // A fresh pool at the same spot does not take it.
        let mut fresh = [None; SLOTS];
        assign_patches(&mut fresh, &[c], at(&c, lx - 30.0, 8.5 * PATCH_SIZE), ranges(RANGE));
        assert!(!keys(&fresh).contains(&target));
    }

    #[test]
    fn neighbours_across_the_cell_border_always_count_as_missing() {
        // Camera on the border between cells 1 (west) and 2 (east).
        let west = cell(1, 0, 0);
        let east = cell(2, 1, 0);
        let mut slots = [None; SLOTS];
        assign_patches(&mut slots, &[west, east], at(&west, CELL, 8.5 * PATCH_SIZE), ranges(RANGE));
        let edge = slots
            .iter()
            .flatten()
            .find(|a| a.key == PatchKey { cell: 1, col: 15, row: 8 })
            .expect("the border square is resident");
        // East neighbours are in cell 2: missing, though cell 2's patches exist.
        for dr in -1..=1 {
            assert_ne!(edge.missing_mask & neighbour_bit(dr, 1), 0);
        }
        // Its west neighbour (14, 8) is resident and not missing.
        assert_eq!(edge.missing_mask & neighbour_bit(0, -1), 0);
        assert!(keys(&slots).iter().any(|k| k.cell == 2));
    }

    #[test]
    fn a_full_pool_keeps_the_nearest_squares() {
        // Range wide enough to want far more than 24 squares.
        let c = cell(1, 0, 0);
        let centre = (8.5 * PATCH_SIZE, 8.5 * PATCH_SIZE);
        let mut slots = [None; SLOTS];
        assign_patches(&mut slots, &[c], at(&c, centre.0, centre.1), ranges(4.0 * PATCH_SIZE));
        assert_eq!(slots.iter().flatten().count(), SLOTS);
        let worst_held = slots
            .iter()
            .flatten()
            .map(|a| distance_to_square(centre.0, centre.1, a.key.col as i32, a.key.row as i32))
            .fold(0.0f32, f32::max);
        for row in 0..PER_SIDE {
            for col in 0..PER_SIDE {
                let key = PatchKey { cell: 1, col: col as u32, row: row as u32 };
                if !keys(&slots).contains(&key) {
                    assert!(distance_to_square(centre.0, centre.1, col, row) >= worst_held);
                }
            }
        }
        // And a patch at the pool's edge has a missing neighbour flagged.
        assert!(slots.iter().flatten().any(|a| a.missing_mask != 0));
    }

    #[test]
    fn an_unloaded_cell_releases_its_patches() {
        let c = cell(1, 0, 0);
        let mut slots = [None; SLOTS];
        assign_patches(&mut slots, &[c], at(&c, 2000.0, 2000.0), ranges(RANGE));
        assert!(slots.iter().flatten().count() > 0);
        assign_patches(&mut slots, &[], at(&c, 2000.0, 2000.0), ranges(RANGE));
        assert_eq!(slots.iter().flatten().count(), 0);
    }

    #[test]
    fn the_shader_parameters_are_the_metre_settings_in_world_units() {
        let state = TerrainPatchState::default();
        let p = state.params();
        assert!((p.amplitude - 0.03 * UNITS_PER_M).abs() < 1e-5);
        assert!((p.lip - 0.001 * UNITS_PER_M).abs() < 1e-6);
        assert!((p.fade_end - 6.0 * UNITS_PER_M).abs() < 1e-4);
        assert!((p.fade_start - 3.0 * UNITS_PER_M).abs() < 1e-4);
        // §3.4: the ramp must fit inside one patch.
        assert!(p.edge_ramp < PATCH_SIZE);
        // Patches keep the parallax march until 4.3 splits the relief.
        assert!(p.parallax);
    }

    #[test]
    fn near_squares_draw_fine_far_ones_coarse_and_fine_borders_see_coarse_neighbours() {
        let c = cell(1, 0, 0);
        let (lx, lz) = (8.5 * PATCH_SIZE, 8.5 * PATCH_SIZE); // mid-square
        let mut slots = [None; SLOTS];
        assign_patches(&mut slots, &[c], at(&c, lx, lz), ranges(RANGE));
        let fine_range = DEFAULT_FINE_RANGE_M * UNITS_PER_M;
        let mut fine = 0;
        for a in slots.iter().flatten() {
            let d = distance_to_square(lx, lz, a.key.col as i32, a.key.row as i32);
            assert_eq!(a.coarse, d > fine_range, "square ({}, {}) at {d}", a.key.col, a.key.row);
            fine += usize::from(!a.coarse);
            // Every resident neighbour is either fine or flagged coarse, never both
            // coarse and missing.
            assert_eq!(a.coarse_mask & a.missing_mask, 0);
        }
        // 2 m from the centre of a 3.66 m square: it and its 4 edge neighbours.
        assert_eq!(fine, 5);
        let centre = slots
            .iter()
            .flatten()
            .find(|a| (a.key.col, a.key.row) == (8, 8))
            .unwrap();
        // Its edge neighbours (1.83 m) are fine, its corner ones (2.59 m) coarse.
        let edges = neighbour_bit(-1, 0) | neighbour_bit(1, 0) | neighbour_bit(0, -1) | neighbour_bit(0, 1);
        assert_eq!(centre.coarse_mask & edges, 0);
        assert_eq!(centre.coarse_mask, 0b111_101_111 & !edges);
        let east = slots
            .iter()
            .flatten()
            .find(|a| (a.key.col, a.key.row) == (9, 8))
            .unwrap();
        assert!(!east.coarse);
        assert_ne!(east.coarse_mask & neighbour_bit(0, 1), 0, "(10, 8) is coarse");
    }

    #[test]
    fn a_fine_patch_stays_fine_inside_the_fine_hysteresis() {
        let c = cell(1, 0, 0);
        let fine_range = DEFAULT_FINE_RANGE_M * UNITS_PER_M;
        let target = PatchKey { cell: 1, col: 10, row: 8 };
        let level = |slots: &[Option<PatchAssignment>; SLOTS]| {
            slots.iter().flatten().find(|a| a.key == target).map(|a| a.coarse)
        };
        // Camera just inside the fine range of square (10, 8)'s west edge.
        let lx = 10.0 * PATCH_SIZE - fine_range + 5.0;
        let lz = 8.5 * PATCH_SIZE;
        let mut slots = [None; SLOTS];
        assign_patches(&mut slots, &[c], at(&c, lx, lz), ranges(RANGE));
        assert_eq!(level(&slots), Some(false));
        // 20 units past the fine range: still fine.
        assign_patches(&mut slots, &[c], at(&c, lx - 25.0, lz), ranges(RANGE));
        assert_eq!(level(&slots), Some(false));
        // A fresh pool there starts it coarse; past the hysteresis it drops.
        let mut fresh = [None; SLOTS];
        assign_patches(&mut fresh, &[c], at(&c, lx - 25.0, lz), ranges(RANGE));
        assert_eq!(level(&fresh), Some(true));
        let past = FINE_HYSTERESIS_M * UNITS_PER_M + 10.0;
        assign_patches(&mut slots, &[c], at(&c, lx - 5.0 - past, lz), ranges(RANGE));
        assert_eq!(level(&slots), Some(true));
    }

    #[test]
    fn a_zero_fine_range_makes_every_patch_coarse_and_a_full_one_every_patch_fine() {
        let c = cell(1, 0, 0);
        let cam = at(&c, 8.5 * PATCH_SIZE, 8.5 * PATCH_SIZE);
        let mut all_coarse = ranges(RANGE);
        all_coarse.fine_range = 0.0;
        let mut slots = [None; SLOTS];
        assign_patches(&mut slots, &[c], cam, all_coarse);
        assert!(slots.iter().flatten().all(|a| a.coarse));
        let mut all_fine = ranges(RANGE);
        all_fine.fine_range = RANGE + HYST;
        let mut slots = [None; SLOTS];
        assign_patches(&mut slots, &[c], cam, all_fine);
        assert!(slots.iter().flatten().all(|a| !a.coarse && a.coarse_mask == 0));
    }

    #[test]
    fn squares_in_a_hidden_quadrant_get_no_patch_and_their_neighbours_fade() {
        // Camera on the cell centre, where all four quadrants meet; hide NE (bit 3).
        let mut c = cell(1, 0, 0);
        c.hidden_quadrants = 1 << 3;
        let cam = at(&c, 8.0 * PATCH_SIZE, 8.0 * PATCH_SIZE);
        let mut slots = [None; SLOTS];
        assign_patches(&mut slots, &[c], cam, ranges(RANGE));
        let held = keys(&slots);
        assert!(!held.is_empty());
        // NE is col >= 8 and row >= 8 (rows grow north, toward -Z).
        assert!(held.iter().all(|k| !(k.col >= 8 && k.row >= 8)), "{held:?}");
        // The other three quadrants still fill the 4×4 block around the centre.
        assert_eq!(held.len(), 12);
        // SE square (8, 7) borders the hidden (8, 8): that side is missing.
        let se = slots
            .iter()
            .flatten()
            .find(|a| (a.key.col, a.key.row) == (8, 7))
            .expect("(8, 7) is resident");
        assert_ne!(se.missing_mask & neighbour_bit(1, 0), 0);
    }

    #[test]
    fn the_quadrant_split_matches_the_land_mesh() {
        // Square (col, row) covers LAND quads 2col..2col+2 × 2row..2row+2;
        // `land_tile_indices` puts LAND quad (qc, qr) in quadrant
        // (qr >= 16) * 2 + (qc >= 16).
        for q in 0..4u8 {
            for row in 0..PER_SIDE {
                for col in 0..PER_SIDE {
                    let land_q = u8::from(2 * row >= 16) * 2 + u8::from(2 * col >= 16);
                    assert_eq!(square_hidden(1 << q, col, row), land_q == q, "q{q} ({col},{row})");
                }
            }
        }
        assert!(!square_hidden(0, 3, 12));
        assert!(square_hidden(0x0F, 3, 12));
    }
}
