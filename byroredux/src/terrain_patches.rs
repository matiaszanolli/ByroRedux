//! TSD-4.1 terrain displacement patches, host side (#7152).
//!
//! Design: `docs/engine/terrain-displacement.md` §3. The renderer owns the
//! pool of reserved patch meshes and the compute pass that displaces them
//! (`byroredux_renderer::vulkan::terrain_patch`); this module decides every
//! frame which 256-unit squares of which resident LAND cells hold a patch, and
//! hands that to the two consumers:
//!
//! - [`emit_patch_draws`] (inside `build_render_data`, before the sort) clones
//!   each patch's base cell draw command onto the patch mesh. It keeps the
//!   base's `in_tlas` (unless `terrain.patches rt off`): the renderer gives each patch mesh its own refit BLAS
//!   and admits the draw to the TLAS once that BLAS matches the frame (#7221);
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
//! Each patch also picks its lattice level, the fine 5.7 cm lattice or the
//! coarse 11.4 cm one (a quarter of the triangles). By default (§7.3, 4.3) a
//! square draws fine while one fine-lattice triangle at its nearest point
//! projects to at least `a_fine` render pixels, from the render extent, the
//! camera's `fov_y`, the 3D distance and the view angle to the square's base
//! surface ([`SquarePlane`]). Only position enters it, so turning the camera
//! never changes a level (each change costs a BLAS build). The cost of the
//! patches follows that projected size, not world distance: small triangles
//! split more 2×2 quads, and each split quad still fills a warp lane (§8.3).
//! `terrain.patches fine <m>` switches to 4.1's world-distance rule.

use byroredux_core::ecs::{EntityId, MeshHandle, Resource, World};
use byroredux_core::lighting::BETHESDA_UNITS_PER_METER as UNITS_PER_M;
use byroredux_core::math::coord::{LAND_GRID_VERTS, LAND_VERTEX_SPACING};
use byroredux_core::math::{Mat4, Vec3, Vec4};
use byroredux_renderer::shader_constants::{
    TERRAIN_PATCHES_PER_CELL_SIDE, TERRAIN_PATCH_LATTICE_STEP, TERRAIN_PATCH_QUADS,
    TERRAIN_PATCH_SLOTS,
};
use byroredux_renderer::vulkan::terrain_patch::{
    neighbour_bit, TerrainPatchMeshes, TerrainPatchParams, TerrainPatchSlot, TerrainPatchStats,
};
use byroredux_renderer::DrawCommand;

use crate::components::{TerrainCellOrigin, TerrainSquarePlanes, TerrainTileSlot};

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
/// The screen-space rule's threshold (§7.3): a square draws fine while one
/// fine-lattice triangle at its nearest point projects to at least this many
/// render pixels. A fine square drops to coarse below half of it. 128 is the
/// sweep's pick (§8.5): at the bench's 480 render lines it selects the squares
/// 4.1's 2 m rule did, and at 960 lines 32–64 px² added fine squares for up to
/// 0.9 ms with nothing measured in return.
pub(crate) const DEFAULT_A_FINE_PX2: f32 = 128.0;
/// Squares in one cell's grid.
pub(crate) const SQUARES_PER_CELL: usize = (PER_SIDE * PER_SIDE) as usize;
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

/// How a patch picks its lattice level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LevelRule {
    /// 4.1's rule: fine within `fine_range` (XZ, world units), kept until
    /// `fine_hysteresis` past it. `fine_range` 0 makes every patch coarse.
    Distance { fine_range: f32, fine_hysteresis: f32 },
    /// §7.3: fine while one fine-lattice triangle at the square's nearest point
    /// projects to at least `a_fine` px² ([`fine_triangle_area_px`]), kept until
    /// it falls below half of that. `px_per_unit` is the projection's pixels per
    /// world unit at unit depth ([`crate::render::px_per_unit_at_unit_depth`]).
    Screen { px_per_unit: f32, a_fine: f32 },
}

/// The level rule as the console sets it (metres / px²); [`TerrainPatchState::ranges`]
/// turns it into a [`LevelRule`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LevelSetting {
    Distance { fine_range_m: f32 },
    Screen { a_fine_px2: f32 },
}

/// The distances (world units) and level rule `assign_patches` works to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PatchRanges {
    pub(crate) range: f32,
    pub(crate) hysteresis: f32,
    pub(crate) level: LevelRule,
}

/// A patch square's base surface (§7.3): the least-squares plane through the
/// nine LAND vertices it covers, in Y-up world units, and their height range.
/// The host knows each square in XZ only otherwise; the screen-space level
/// rule needs the 3D distance and the view angle to the ground, and a slope
/// facing the camera (FNV's spawn) is exactly where a flat-ground guess fails.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct SquarePlane {
    /// The plane's height at the square's centre (its centre vertex's XZ).
    pub(crate) y_centre: f32,
    /// dY/dX and dY/dZ (Y-up Z) of the plane.
    pub(crate) slope: [f32; 2],
    pub(crate) y_min: f32,
    pub(crate) y_max: f32,
}

/// Fit every square of a LAND tile: `heights` is the 33×33 Z-up height grid,
/// row-major, rows toward −Z in Y-up (`cell_loader::terrain`'s mapping), each
/// value already sanitized. `None` if the grid is not 33×33. Square
/// (`col`, `row`) covers vertices `2col..=2col+2` × `2row..=2row+2`.
pub(crate) fn square_planes(heights: &[f32]) -> Option<Box<[SquarePlane; SQUARES_PER_CELL]>> {
    const GRID: usize = LAND_GRID_VERTS;
    if heights.len() != GRID * GRID {
        return None;
    }
    let s = LAND_VERTEX_SPACING;
    // Least squares on a symmetric 3×3 design: the intercept is the mean, and
    // each slope is Σ h·d / Σ d² with Σ d² = 6 s².
    let denom = 6.0 * s * s;
    let mut planes = Box::new([SquarePlane::default(); SQUARES_PER_CELL]);
    for row in 0..PER_SIDE as usize {
        for col in 0..PER_SIDE as usize {
            let (mut sum, mut sx, mut sz) = (0.0f32, 0.0f32, 0.0f32);
            let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
            for j in 0..3 {
                for i in 0..3 {
                    let h = heights[(2 * row + j) * GRID + 2 * col + i];
                    let dx = (i as f32 - 1.0) * s;
                    // Rows run toward −Z in Y-up.
                    let dz = -(j as f32 - 1.0) * s;
                    sum += h;
                    sx += h * dx;
                    sz += h * dz;
                    lo = lo.min(h);
                    hi = hi.max(h);
                }
            }
            planes[row * PER_SIDE as usize + col] = SquarePlane {
                y_centre: sum / 9.0,
                slope: [sx / denom, sz / denom],
                y_min: lo,
                y_max: hi,
            };
        }
    }
    Some(planes)
}

/// Projected area, in render pixels, of one fine-lattice triangle (half a
/// 4-unit quad) at square (`col`, `row`)'s nearest point to the camera (§7.3).
/// `cam_local` is the camera in cell-local terms: `lx` east, `y` world height,
/// `lz` toward −Z. The view angle to the ground comes from the square's base
/// `plane`; without one the surface is taken as facing the camera at the XZ
/// distance, which over-estimates the area (biased toward fine).
fn fine_triangle_area_px(
    cam_local: [f32; 3],
    col: i32,
    row: i32,
    plane: Option<SquarePlane>,
    px_per_unit: f32,
) -> f32 {
    let [lx, cam_y, lz] = cam_local;
    let (x0, z0) = (col as f32 * PATCH_SIZE, row as f32 * PATCH_SIZE);
    let (px, pz) = (lx.clamp(x0, x0 + PATCH_SIZE), lz.clamp(z0, z0 + PATCH_SIZE));
    // Camera minus nearest point, Y-up (local `lz` runs toward −Z).
    let (vx, vz) = (lx - px, -(lz - pz));
    let (vy, sin_view) = match plane {
        Some(p) => {
            let half = 0.5 * PATCH_SIZE;
            let (dx, dz) = (px - (x0 + half), -(pz - (z0 + half)));
            let y = (p.y_centre + p.slope[0] * dx + p.slope[1] * dz).clamp(p.y_min, p.y_max);
            let vy = cam_y - y;
            let n = [-p.slope[0], 1.0, -p.slope[1]];
            let n_len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            let d = (vx * vx + vy * vy + vz * vz).sqrt();
            let sin_view = if d > 0.0 {
                ((n[0] * vx + n[1] * vy + n[2] * vz) / (n_len * d)).abs()
            } else {
                1.0
            };
            (vy, sin_view)
        }
        None => (0.0, 1.0),
    };
    let d = (vx * vx + vy * vy + vz * vz).sqrt();
    if d <= f32::EPSILON {
        return f32::INFINITY;
    }
    let edge_px = TERRAIN_PATCH_LATTICE_STEP * px_per_unit / d;
    0.5 * edge_px * edge_px * sin_view
}

/// The pool's host-side state.
pub(crate) struct TerrainPatchState {
    /// `terrain.patches off` turns the whole stage off (the A/B switch).
    pub(crate) enabled: bool,
    pub(crate) amplitude_m: f32,
    pub(crate) range_m: f32,
    /// The lattice-level rule (`terrain.patches afine <px²>` / `fine <m>`).
    pub(crate) level: LevelSetting,
    /// The scene-render extent's height (`VulkanContext::render_extent`, the
    /// pixels the main pass shades, below the output under an upscaler) and
    /// the camera's vertical field of view, both from the last frame. Until
    /// both are known the level falls back to the distance rule.
    pub(crate) render_height: u32,
    pub(crate) fov_y: f32,
    /// Keep the TSD-3 parallax march on patch draws (default on; see
    /// `TerrainPatchParams::parallax`).
    pub(crate) parallax: bool,
    /// Admit patch draws to the TLAS (default on). `terrain.patches rt off`
    /// keeps them raster-only — rays then see the base terrain under them — so
    /// the ray-traversal share of the patch cost can be measured on its own
    /// (#7221).
    pub(crate) rt: bool,
    /// The renderer's reserved patch meshes, indexed by slot; copied on the
    /// first frame with a renderer. Empty means no patches.
    pub(crate) meshes: TerrainPatchMeshes,
    pub(crate) slots: [Option<PatchAssignment>; SLOTS],
    /// The camera position the slots were assigned from, world units.
    pub(crate) camera_pos: [f32; 3],
    /// The renderer's last seam check, for `terrain.patches`.
    pub(crate) stats: TerrainPatchStats,
    /// Per-frame working lists, kept across frames (#7263).
    scratch: PatchScratch,
}

/// The lists [`emit_patch_draws`] and [`push_terrain_patches`] rebuild every
/// exterior frame, kept on [`TerrainPatchState`] so a frame with resident
/// patches allocates nothing (#7263, the host half of #7246's fix).
#[derive(Default)]
struct PatchScratch {
    cells: Vec<PatchCell>,
    candidates: Vec<(f32, PatchKey, PatchCell)>,
    frame_slots: Vec<TerrainPatchSlot>,
}

impl Default for TerrainPatchState {
    fn default() -> Self {
        Self {
            enabled: true,
            amplitude_m: DEFAULT_AMPLITUDE_M,
            range_m: DEFAULT_RANGE_M,
            level: LevelSetting::Screen { a_fine_px2: DEFAULT_A_FINE_PX2 },
            render_height: 0,
            fov_y: 0.0,
            parallax: true,
            rt: true,
            meshes: TerrainPatchMeshes::default(),
            slots: [None; SLOTS],
            camera_pos: [0.0; 3],
            stats: TerrainPatchStats::default(),
            scratch: PatchScratch::default(),
        }
    }
}

impl Resource for TerrainPatchState {}

impl TerrainPatchState {
    pub(crate) fn resident(&self) -> usize {
        self.slots.iter().flatten().count()
    }

    pub(crate) fn ranges(&self) -> PatchRanges {
        let distance = |fine_range_m: f32| LevelRule::Distance {
            fine_range: fine_range_m * UNITS_PER_M,
            fine_hysteresis: FINE_HYSTERESIS_M * UNITS_PER_M,
        };
        let level = match self.level {
            LevelSetting::Distance { fine_range_m } => distance(fine_range_m),
            LevelSetting::Screen { a_fine_px2 } => match self.px_per_unit() {
                Some(px_per_unit) => LevelRule::Screen { px_per_unit, a_fine: a_fine_px2 },
                None => distance(DEFAULT_FINE_RANGE_M),
            },
        };
        PatchRanges {
            range: self.range_m * UNITS_PER_M,
            hysteresis: HYSTERESIS_M * UNITS_PER_M,
            level,
        }
    }

    /// The screen-space rule's projection scale, once the render height and
    /// the camera's field of view are known.
    pub(crate) fn px_per_unit(&self) -> Option<f32> {
        (self.render_height > 0 && self.fov_y > 0.0)
            .then(|| crate::render::px_per_unit_at_unit_depth(self.render_height, self.fov_y))
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

/// [`assign_patches_with`] on a fresh candidate list, a camera at height 0
/// and no base planes.
#[cfg(test)]
pub(crate) fn assign_patches(
    slots: &mut [Option<PatchAssignment>; SLOTS],
    cells: &[PatchCell],
    cam_xz: [f32; 2],
    ranges: PatchRanges,
) {
    assign_patches_with(&mut Vec::new(), slots, cells, [cam_xz[0], 0.0, cam_xz[1]], ranges, &|_, _, _| None);
}

/// Reassign `slots` for a camera at `cam` (Y-up world units), keeping every
/// patch that is still wanted in the slot it already has, and pick each
/// patch's lattice level. `candidates` is the caller's kept working list;
/// `plane` looks up a square's base surface (`cell`, `col`, `row`), which the
/// screen-space level rule reads.
fn assign_patches_with(
    candidates: &mut Vec<(f32, PatchKey, PatchCell)>,
    slots: &mut [Option<PatchAssignment>; SLOTS],
    cells: &[PatchCell],
    cam: [f32; 3],
    ranges: PatchRanges,
    plane: &dyn Fn(EntityId, i32, i32) -> Option<SquarePlane>,
) {
    let PatchRanges { range, hysteresis, level } = ranges;
    let cam_xz = [cam[0], cam[2]];
    let keep_range = range + hysteresis;
    candidates.clear();
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
    for &(_, key, cell) in candidates.iter() {
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

    // Level, with hysteresis toward the current one (a new patch starts
    // coarse). Position only: the camera's orientation never enters.
    for assignment in slots.iter_mut().flatten() {
        let lx = cam_xz[0] - assignment.cell.origin_xz[0];
        let lz = assignment.cell.origin_xz[1] - cam_xz[1];
        let (col, row) = (assignment.key.col as i32, assignment.key.row as i32);
        let was_fine = !assignment.coarse;
        let fine = match level {
            LevelRule::Distance { fine_range, fine_hysteresis } => {
                let dist = distance_to_square(lx, lz, col, row);
                fine_range > 0.0
                    && (dist <= fine_range || (was_fine && dist <= fine_range + fine_hysteresis))
            }
            LevelRule::Screen { px_per_unit, a_fine } => {
                let area = fine_triangle_area_px(
                    [lx, cam[1], lz],
                    col,
                    row,
                    plane(assignment.key.cell, col, row),
                    px_per_unit,
                );
                area >= a_fine || (was_fine && area >= 0.5 * a_fine)
            }
        };
        assignment.coarse = !fine;
    }

    // A neighbour square is missing unless a resident patch of the same cell
    // holds it; squares across the cell border always are (the cell-border
    // rule, §3.4).
    let resident = *slots;
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
                match resident.iter().flatten().find(|a| inside && a.key == key) {
                    None => mask |= neighbour_bit(dr, dc),
                    Some(a) if a.coarse => coarse_mask |= neighbour_bit(dr, dc),
                    Some(_) => {}
                }
            }
        }
        assignment.missing_mask = mask;
        assignment.coarse_mask = coarse_mask;
    }
}

/// Every resident full-detail LAND cell within reach of `cam_xz`, into
/// `cells` (cleared first).
fn nearby_cells(world: &World, cam_xz: [f32; 2], reach: f32, cells: &mut Vec<PatchCell>) {
    cells.clear();
    // #7314 — `MeshHandle` before `TerrainTileSlot`: the static-mesh collector and cell unload
    // hold them in that order, and a tuple acquires left to right with every guard kept.
    let (Some(origins), Some(meshes), Some(tiles)) = (
        world.query::<TerrainCellOrigin>(),
        world.query::<MeshHandle>(),
        world.query::<TerrainTileSlot>(),
    ) else {
        return;
    };
    let cell_size = PATCH_SIZE * PER_SIDE as f32;
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
}

/// Assign this frame's patches and append their draws: each patch draws as a
/// copy of its base cell's command with the patch mesh swapped in, TLAS
/// membership included (the renderer resolves the patch BLAS, #7221). Called
/// before the draw sort; the patch
/// meshes are the registry's first handles, so in the opaque sort (grouped by
/// mesh) every patch draws before any base cell and early-Z rejects the base
/// under it.
pub(crate) fn emit_patch_draws(
    world: &World,
    cam_pos: Vec3,
    vp_mat: Mat4,
    fov_y: f32,
    draw_commands: &mut Vec<DrawCommand>,
) {
    let Some(mut state) = world.try_resource_mut::<TerrainPatchState>() else {
        return;
    };
    let state = &mut *state;
    state.camera_pos = cam_pos.to_array();
    state.fov_y = fov_y;
    if !state.enabled || state.meshes.fine.len() < SLOTS || state.meshes.coarse.len() < SLOTS {
        state.slots = [None; SLOTS];
        return;
    }
    let ranges = state.ranges();
    let cam_xz = [cam_pos.x, cam_pos.z];
    let scratch = &mut state.scratch;
    nearby_cells(world, cam_xz, ranges.range + ranges.hysteresis, &mut scratch.cells);
    let planes = world.query::<TerrainSquarePlanes>();
    let plane = |cell: EntityId, col: i32, row: i32| {
        let planes = planes.as_ref()?.get(cell)?;
        planes.0.get((row * PER_SIDE + col) as usize).copied()
    };
    assign_patches_with(
        &mut scratch.candidates,
        &mut state.slots,
        &scratch.cells,
        cam_pos.to_array(),
        ranges,
        &plane,
    );

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
        patch.in_tlas = base.in_tlas && state.rt;
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
    if state.meshes.is_empty()
        && let Some(meshes) = ctx.terrain_patch_meshes()
    {
        state.meshes = meshes.clone();
    }
    if let Some(stats) = ctx.terrain_patch_stats() {
        state.stats = stats.clone();
    }
    // The pixels the main pass shades, for next frame's level rule (§7.3).
    state.render_height = ctx.render_extent().1;
    let state = &mut *state;
    let params = state.params();
    let slots = &mut state.scratch.frame_slots;
    slots.clear();
    slots.extend(state
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
        );
    if slots.is_empty() {
        return;
    }
    ctx.set_terrain_patch_frame(slots, params);
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

    /// 4.1's distance rule at its defaults: the tests below that predate the
    /// screen-space rule pin the forced `fine <m>` mode.
    fn distance(fine_range: f32) -> LevelRule {
        LevelRule::Distance { fine_range, fine_hysteresis: FINE_HYSTERESIS_M * UNITS_PER_M }
    }

    fn ranges(range: f32) -> PatchRanges {
        PatchRanges { range, hysteresis: HYST, level: distance(DEFAULT_FINE_RANGE_M * UNITS_PER_M) }
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
        // Patches keep the parallax march by default; since 4.3 it covers only the residual depth (§3.6).
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
        all_coarse.level = distance(0.0);
        let mut slots = [None; SLOTS];
        assign_patches(&mut slots, &[c], cam, all_coarse);
        assert!(slots.iter().flatten().all(|a| a.coarse));
        let mut all_fine = ranges(RANGE);
        all_fine.level = distance(RANGE + HYST);
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

    /// #7263 — the per-frame candidate list is the caller's: a second frame
    /// refills the first frame's allocation, and the assignment is the same as
    /// on a fresh list.
    #[test]
    fn the_candidate_list_keeps_its_allocation_across_frames() {
        let c = cell(1, 0, 0);
        let cam = at(&c, 100.0, 100.0);
        let mut fresh = [None; SLOTS];
        assign_patches(&mut fresh, &[c], cam, ranges(RANGE));

        let mut candidates = Vec::new();
        let mut kept = [None; SLOTS];
        let cam3 = [cam[0], 0.0, cam[1]];
        assign_patches_with(&mut candidates, &mut kept, &[c], cam3, ranges(RANGE), &|_, _, _| None);
        assert!(!candidates.is_empty(), "the camera's own cell offers squares");
        let before = (candidates.as_ptr(), candidates.capacity());
        assign_patches_with(&mut candidates, &mut kept, &[c], cam3, ranges(RANGE), &|_, _, _| None);
        assert_eq!((candidates.as_ptr(), candidates.capacity()), before, "the second frame reuses the list");
        assert_eq!(kept, fresh, "a kept list assigns exactly what a fresh one does");
    }

    // ── §7.3 screen-space level rule ─────────────────────────────────────

    /// 480 render lines at a 45° vertical field of view: §7.3's table.
    fn k480() -> f32 {
        crate::render::px_per_unit_at_unit_depth(480, 45f32.to_radians())
    }

    /// A Z-up LAND height grid of the plane `y = base + gx·x + gz·z` (Y-up, cell-local).
    fn tilted_grid(base: f32, gx: f32, gz: f32) -> Vec<f32> {
        let s = LAND_VERTEX_SPACING;
        let mut h = Vec::with_capacity(LAND_GRID_VERTS * LAND_GRID_VERTS);
        for row in 0..LAND_GRID_VERTS {
            for col in 0..LAND_GRID_VERTS {
                // Rows run toward −Z in Y-up.
                h.push(base + gx * col as f32 * s + gz * -(row as f32 * s));
            }
        }
        h
    }

    #[test]
    fn a_square_is_two_land_quads_across() {
        assert_eq!(PATCH_SIZE, 2.0 * LAND_VERTEX_SPACING);
        assert_eq!(2 * PER_SIDE as usize + 1, LAND_GRID_VERTS);
    }

    #[test]
    fn the_square_planes_fit_a_tilted_grid_exactly() {
        let planes = square_planes(&tilted_grid(100.0, 0.25, 0.5)).expect("33×33 grid");
        let s = LAND_VERTEX_SPACING;
        for (row, col) in [(0usize, 0usize), (7, 3), (15, 15)] {
            let p = planes[row * PER_SIDE as usize + col];
            let (cx, cz) = ((2 * col + 1) as f32 * s, -((2 * row + 1) as f32 * s));
            assert!((p.y_centre - (100.0 + 0.25 * cx + 0.5 * cz)).abs() < 1e-2, "{p:?}");
            assert!((p.slope[0] - 0.25).abs() < 1e-5 && (p.slope[1] - 0.5).abs() < 1e-5, "{p:?}");
            // The range is the corners' (a plane's extremes on a square).
            assert!((p.y_max - p.y_min - (0.25 + 0.5) * 2.0 * s).abs() < 1e-2, "{p:?}");
        }
        assert!(square_planes(&[0.0; 10]).is_none(), "not a 33×33 grid");
    }

    #[test]
    fn the_projected_area_matches_the_design_note_on_flat_ground() {
        // §7.3: eye 1.7 m over flat ground, a fine triangle 2 m away is ~51 px²
        // at 480 lines; 6 m away, ~3.8 px².
        let flat = SquarePlane { y_centre: 0.0, slope: [0.0, 0.0], y_min: 0.0, y_max: 0.0 };
        let eye = 1.7 * UNITS_PER_M;
        for (metres, expect) in [(2.0, 51.3), (6.0, 3.8)] {
            // Camera west of square (8, 8) by `metres`, level with its centre row.
            let lx = 8.0 * PATCH_SIZE - metres * UNITS_PER_M;
            let area = fine_triangle_area_px([lx, eye, 8.5 * PATCH_SIZE], 8, 8, Some(flat), k480());
            assert!((area - expect).abs() < 0.05 * expect, "{metres} m: {area} px², expected {expect}");
        }
        // Straight down onto the square: no foreshortening, distance = eye height.
        let below = fine_triangle_area_px([8.5 * PATCH_SIZE, eye, 8.5 * PATCH_SIZE], 8, 8, Some(flat), k480());
        let edge = TERRAIN_PATCH_LATTICE_STEP * k480() / eye;
        assert!((below - 0.5 * edge * edge).abs() < 1e-3 * below);
    }

    #[test]
    fn a_slope_facing_the_camera_projects_larger_than_flat_ground() {
        // Camera 2 m west of square (8, 8) at eye height; the ground rising
        // toward +X (away from the camera, so facing it) at 30°.
        let eye = 1.7 * UNITS_PER_M;
        let lx = 8.0 * PATCH_SIZE - 2.0 * UNITS_PER_M;
        let near_y = 0.0;
        let rise = 30f32.to_radians().tan();
        // Plane through height 0 at the square's west edge.
        let facing = SquarePlane {
            y_centre: near_y + rise * 0.5 * PATCH_SIZE,
            slope: [rise, 0.0],
            y_min: near_y,
            y_max: near_y + rise * PATCH_SIZE,
        };
        let flat = SquarePlane { y_centre: 0.0, slope: [0.0, 0.0], y_min: 0.0, y_max: 0.0 };
        let cam = [lx, eye, 8.5 * PATCH_SIZE];
        let a_facing = fine_triangle_area_px(cam, 8, 8, Some(facing), k480());
        let a_flat = fine_triangle_area_px(cam, 8, 8, Some(flat), k480());
        // Same nearest point and distance, so the ratio is the view-angle term:
        // n·v̂ with n = (−sin 30°, cos 30°) and v = (−2 m, 1.7 m), against 1.7 / D.
        let (h, w) = (1.7f32, 2.0f32);
        let expect = (30f32.to_radians().sin() * w + 30f32.to_radians().cos() * h) / h;
        assert!((a_facing / a_flat - expect).abs() < 1e-3, "facing {a_facing} vs flat {a_flat}, ratio {expect}");
        assert!(a_facing > a_flat);
        // No plane: the surface is taken as facing the camera (biased to fine).
        let unknown = fine_triangle_area_px(cam, 8, 8, None, k480());
        assert!(unknown >= a_facing, "{unknown} vs {a_facing}");
    }

    fn screen(a_fine: f32, px_per_unit: f32) -> PatchRanges {
        PatchRanges { range: RANGE, hysteresis: HYST, level: LevelRule::Screen { px_per_unit, a_fine } }
    }

    fn assign_screen(
        slots: &mut [Option<PatchAssignment>; SLOTS],
        c: &PatchCell,
        lx: f32,
        lz: f32,
        eye: f32,
        r: PatchRanges,
    ) {
        let flat = SquarePlane { y_centre: 0.0, slope: [0.0, 0.0], y_min: 0.0, y_max: 0.0 };
        let cam = at(c, lx, lz);
        assign_patches_with(&mut Vec::new(), slots, &[*c], [cam[0], eye, cam[1]], r, &|_, _, _| Some(flat));
    }

    #[test]
    fn the_screen_rule_draws_fine_exactly_where_the_triangles_are_large_enough() {
        let c = cell(1, 0, 0);
        let (lx, lz, eye) = (8.5 * PATCH_SIZE, 8.5 * PATCH_SIZE, 1.7 * UNITS_PER_M);
        let mut slots = [None; SLOTS];
        assign_screen(&mut slots, &c, lx, lz, eye, screen(DEFAULT_A_FINE_PX2, k480()));
        let flat = SquarePlane { y_centre: 0.0, slope: [0.0, 0.0], y_min: 0.0, y_max: 0.0 };
        let (mut fine, mut coarse) = (0, 0);
        for a in slots.iter().flatten() {
            let area =
                fine_triangle_area_px([lx, eye, lz], a.key.col as i32, a.key.row as i32, Some(flat), k480());
            // A fresh pool: fine exactly at or above the threshold.
            assert_eq!(!a.coarse, area >= DEFAULT_A_FINE_PX2, "({}, {}) {area} px²", a.key.col, a.key.row);
            assert_eq!(a.coarse_mask & a.missing_mask, 0);
            if a.coarse { coarse += 1 } else { fine += 1 }
        }
        assert!(fine > 0 && coarse > 0, "{fine} fine, {coarse} coarse");
        // Three times the render height: every triangle nine times larger, so
        // at least as many squares fine.
        let mut hi = [None; SLOTS];
        assign_screen(&mut hi, &c, lx, lz, eye, screen(DEFAULT_A_FINE_PX2, 3.0 * k480()));
        let fine_hi = hi.iter().flatten().filter(|a| !a.coarse).count();
        assert!(fine_hi > fine, "{fine_hi} fine at 1440 lines vs {fine} at 480");
    }

    #[test]
    fn a_fine_square_stays_fine_down_to_half_the_threshold() {
        let c = cell(1, 0, 0);
        let target = PatchKey { cell: 1, col: 10, row: 8 };
        let level = |slots: &[Option<PatchAssignment>; SLOTS]| {
            slots.iter().flatten().find(|a| a.key == target).map(|a| a.coarse)
        };
        let (lz, eye) = (8.5 * PATCH_SIZE, 1.7 * UNITS_PER_M);
        let flat = SquarePlane { y_centre: 0.0, slope: [0.0, 0.0], y_min: 0.0, y_max: 0.0 };
        let area_at = |lx: f32| fine_triangle_area_px([lx, eye, lz], 10, 8, Some(flat), k480());
        // Walk west from the square until its area crosses each threshold.
        let mut lx = 10.0 * PATCH_SIZE - 1.0;
        while area_at(lx) >= DEFAULT_A_FINE_PX2 {
            lx -= 1.0;
        }
        let just_under = lx; // below a_fine, above a_fine / 2
        while area_at(lx) >= 0.5 * DEFAULT_A_FINE_PX2 {
            lx -= 1.0;
        }
        let under_half = lx;
        let r = screen(DEFAULT_A_FINE_PX2, k480());
        let mut slots = [None; SLOTS];
        assign_screen(&mut slots, &c, just_under + 20.0, lz, eye, r);
        assert_eq!(level(&slots), Some(false), "above the threshold: fine");
        assign_screen(&mut slots, &c, just_under, lz, eye, r);
        assert_eq!(level(&slots), Some(false), "under the threshold, over half: still fine");
        let mut fresh = [None; SLOTS];
        assign_screen(&mut fresh, &c, just_under, lz, eye, r);
        assert_eq!(level(&fresh), Some(true), "a new patch there starts coarse");
        assign_screen(&mut slots, &c, under_half, lz, eye, r);
        assert_eq!(level(&slots), Some(true), "under half the threshold: coarse");
    }

    #[test]
    fn the_screen_rule_waits_for_the_render_height_and_field_of_view() {
        let mut state = TerrainPatchState::default();
        assert_eq!(state.level, LevelSetting::Screen { a_fine_px2: DEFAULT_A_FINE_PX2 });
        assert_eq!(state.ranges().level, distance(DEFAULT_FINE_RANGE_M * UNITS_PER_M));
        state.render_height = 480;
        state.fov_y = 45f32.to_radians();
        match state.ranges().level {
            LevelRule::Screen { px_per_unit, a_fine } => {
                assert!((px_per_unit - k480()).abs() < 1e-3);
                assert_eq!(a_fine, DEFAULT_A_FINE_PX2);
            }
            other => panic!("{other:?}"),
        }
        state.level = LevelSetting::Distance { fine_range_m: 0.0 };
        assert_eq!(state.ranges().level, distance(0.0));
    }

    /// #7314 — the static-mesh collector (`StaticMeshQueries::new`) and cell unload
    /// (`collect_victim_gpu_handles`) hold `MeshHandle` while they take `TerrainTileSlot`;
    /// `nearby_cells` must take the two in that order too. No patch fixture had all three of its
    /// storages, so the lock-order lane (`BYRO_LOCK_ORDER_CHECK=1`) never saw both edges. The
    /// collector's edge is taken here the way it takes it, then the host gathers.
    #[test]
    fn nearby_cells_takes_mesh_handle_before_terrain_tile_slot() {
        let mut world = World::new();
        let cell = world.spawn();
        world.insert(cell, TerrainCellOrigin { origin_xz: [0.0, 0.0], hidden_quadrants: 0 });
        world.insert(cell, TerrainTileSlot(3));
        world.insert(cell, MeshHandle(9));
        {
            // The collector: `MeshHandle` held, then `TerrainTileSlot`.
            let meshes = world.query::<MeshHandle>();
            let _tiles = world.query::<TerrainTileSlot>();
            drop(meshes);
        }
        let mut cells = Vec::new();
        nearby_cells(&world, [100.0, -100.0], RANGE, &mut cells);
        assert_eq!(cells.len(), 1);
        assert_eq!((cells[0].base_mesh, cells[0].tile), (9, 3));
    }
}
