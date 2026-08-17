//! Shared reconciliation policy and geometry helpers for the three
//! distant-LOD spawn paths — terrain, object LOD (`.bto`,
//! [`super::object_lod`]), and placement LOD (`_far.nif`,
//! [`super::placement_lod`]).
//!
//! The format-specific streaming logic (`.bto` vs `.lod` discovery, ring
//! math, per-game gating) is deliberately kept separate in each module.
//! Format-agnostic budget accounting, closest-first ordering, shared inputs,
//! and `ImportedMesh` → renderer conversion live here so providers cannot
//! drift on scheduling, vertex defaults, or bound math (TD2-105 / #2064).

use std::time::Instant;

use byroredux_core::math::Vec3;
use byroredux_nif::import::ImportedMesh;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_renderer::Vertex;

use crate::asset_provider::TextureProvider;
use crate::env_translate::{terrain_lod_layout, TerrainLodLayout};

use super::exterior::ExteriorWorldContext;

/// Immutable inputs shared by terrain and both game-specific object LOD
/// providers during one ring reconciliation.
pub(crate) struct LodReconcileInput<'a> {
    pub(crate) tex_provider: &'a TextureProvider,
    pub(crate) wctx: &'a ExteriorWorldContext,
    pub(crate) player_grid: (i32, i32),
    /// Worldspace-relative cell-grid origin used by Skyrim+/FO4 baked LOD.
    pub(crate) lod_grid_origin: (i32, i32),
    /// The full-detail streaming hysteresis boundary (`radius_unload`).
    pub(crate) max_full_cell_radius: i32,
}

/// Whether `game` ships Bethesda's prebaked **combined** distant LOD — the
/// `.bto` object quads and `.btr` terrain quads named by quadtree level and
/// quad coordinate. Skyrim (LE/SE) and FO4 only. Oblivion and FO3/FNV have
/// authored terrain LOD too, but in their older NIF/DDS layouts; that separate
/// capability is [`legacy_landscape_lod_supported`] (#3100).
///
/// One named decision per quirk, per exal.md §4: the same
/// `Skyrim | Fallout4` literal was previously written inline at each
/// consumer, which is exactly the drift the doctrine forbids — a title
/// gaining or losing baked LOD would have to be found in every provider
/// rather than changed here. [`LodBandLadder::for_game`] answers the same
/// combined-`.btr` question in data form; the tests below pin the two to
/// agree. #2452 / EXAL-04.
pub(crate) fn combined_lod_supported(game: GameKind) -> bool {
    terrain_lod_layout(game) == TerrainLodLayout::Combined
}

/// Whether `game` ships an authored pre-`.btr` landscape LOD family.
///
/// Both generations are translated by EXAL before the synth renderer sees
/// them: Oblivion uses FormID-keyed 32-cell NIF/DDS quads, while FO3/FNV use
/// EditorID-keyed level 4/8/16/32 NIF/DDS quads. This predicate deliberately
/// says nothing about `.bto`/`.btr`; callers needing that combined format use
/// [`combined_lod_supported`] instead (#3100).
pub(crate) fn legacy_landscape_lod_supported(game: GameKind) -> bool {
    matches!(
        terrain_lod_layout(game),
        TerrainLodLayout::OblivionLegacy | TerrainLodLayout::FalloutLegacy
    )
}

/// Canonical baked-LOD grid origin for the active worldspace (#2586).
///
/// Skyrim+/FO4 align `.bto` and `.btr` quad coordinates relative to the
/// worldspace's own minimum cell, not global `(0, 0)`. Prefer WRLD NAM0 via
/// `usable_cell_bounds`; derived/synthetic worldspaces that omit NAM0 fall
/// back to the component-wise minimum of their explicit exterior cells.
/// Older games keep their existing absolute synth/placement grids.
pub(crate) fn worldspace_lod_grid_origin(wctx: &ExteriorWorldContext) -> (i32, i32) {
    if !combined_lod_supported(wctx.record_index.game) {
        return (0, 0);
    }

    let cells = &wctx.record_index.cells;
    if let Some(origin) = cells
        .worldspaces
        .get(&wctx.worldspace_key)
        .and_then(|worldspace| worldspace.usable_cell_bounds())
        .map(|(min, _)| min)
    {
        return origin;
    }

    cells
        .exterior_cells
        .get(&wctx.worldspace_key)
        .and_then(|world_cells| {
            world_cells
                .keys()
                .copied()
                .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1)))
        })
        .unwrap_or((0, 0))
}

/// Inclusive cell bounds of the active worldspace, when it authors enough
/// to know them. Used by `lod_bands` to prune baked-LOD quads that miss the
/// worldspace entirely — without it, the subdivide-on-missing-asset rule
/// would descend open ocean all the way to the finest band.
///
/// Prefers WRLD `NAM0`/`NAM9` (the same source as
/// [`worldspace_lod_grid_origin`]); worldspaces that omit it fall back to the
/// component-wise extent of their explicit exterior cells, and a worldspace
/// with neither returns `None` (prune nothing).
pub(crate) fn worldspace_cell_bounds(
    wctx: &ExteriorWorldContext,
) -> Option<((i32, i32), (i32, i32))> {
    let cells = &wctx.record_index.cells;
    if let Some(bounds) = cells
        .worldspaces
        .get(&wctx.worldspace_key)
        .and_then(|worldspace| worldspace.usable_cell_bounds())
    {
        return Some(bounds);
    }

    cells
        .exterior_cells
        .get(&wctx.worldspace_key)
        .and_then(|world_cells| {
            world_cells.keys().copied().map(|c| (c, c)).reduce(|a, b| {
                (
                    (a.0 .0.min(b.0 .0), a.0 .1.min(b.0 .1)),
                    (a.1 .0.max(b.1 .0), a.1 .1.max(b.1 .1)),
                )
            })
        })
}

/// SW corner of the level-sized quad containing `(gx, gy)`, aligned to the
/// active worldspace's own `grid_origin` rather than global zero (#2586).
pub(crate) fn quad_origin(gx: i32, gy: i32, level: i32, grid_origin: (i32, i32)) -> (i32, i32) {
    (
        grid_origin.0 + (gx - grid_origin.0).div_euclid(level) * level,
        grid_origin.1 + (gy - grid_origin.1).div_euclid(level) * level,
    )
}

/// Bounded main-thread work allowance shared by every distant-LOD provider.
///
/// One unit represents one archive/import/upload attempt for a terrain block,
/// object quad, or placement cell. Reclaims do not consume the budget: stale
/// geometry must disappear immediately when the player moves, while new
/// geometry may fill in progressively over later frames.
///
/// Bounded on **two** axes (EX-07 / #2376):
///
/// * an attempt count, which keeps any one provider from monopolising a
///   frame's units and starving the others, and
/// * a wall-clock `deadline`, which is what actually protects the frame.
///
/// The attempt count alone never did. Attempts are wildly non-uniform: one
/// is an archive extract plus NIF parse plus import plus GPU upload of a
/// macro-mesh whose footprint depends on its LOD band, and since #2371 the
/// coarse bands stream quads covering up to 32×32 cells. Two attempts of
/// that can overrun a frame as easily as twenty cheap ones, and no attempt
/// count can tell the two apart. The deadline can.
///
/// Cooperative, matching [`super::FrameTimeBudget`]: a provider that has not
/// yet completed anything is always admitted once, so an over-budget attempt
/// defers the *next* unit rather than deadlocking the ring.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LodWorkBudget {
    remaining: usize,
    spent: usize,
    deadline: Option<Instant>,
}

impl LodWorkBudget {
    /// Attempt-capped only — no time bound. For deterministic bootstrap and
    /// tests; the per-frame path uses [`Self::with_deadline`].
    pub(crate) fn new(max_attempts: usize) -> Self {
        Self {
            remaining: max_attempts,
            spent: 0,
            deadline: None,
        }
    }

    /// Attempt-capped *and* deadline-bounded. `deadline` is shared across
    /// every provider in one reconcile, so the rings collectively yield at
    /// the frame's streaming allowance instead of each starting a fresh one.
    pub(crate) fn with_deadline(max_attempts: usize, deadline: Instant) -> Self {
        Self {
            remaining: max_attempts,
            spent: 0,
            deadline: Some(deadline),
        }
    }

    pub(crate) fn unlimited() -> Self {
        Self::new(usize::MAX)
    }

    /// Reserve one potentially expensive provider attempt.
    ///
    /// Refuses once either bound is hit — except for the very first attempt,
    /// which is always admitted so the ring makes progress even if the
    /// deadline already elapsed applying full-detail cells.
    pub(crate) fn try_take(&mut self) -> bool {
        if self.remaining == 0 {
            return false;
        }
        if self.spent > 0 && self.deadline.is_some_and(|at| Instant::now() >= at) {
            return false;
        }
        self.remaining -= 1;
        self.spent += 1;
        true
    }

    pub(crate) fn spent(self) -> usize {
        self.spent
    }
}

/// Apply the shared closest-first ordering used by every LOD provider.
///
/// Providers supply their own distance metric (terrain block distance, object
/// quad minimum-cell distance, or placement-cell distance); Y/X tie-breakers
/// keep the result stable across runs and archive/hash iteration order.
pub(crate) fn sort_lod_coords_nearest(
    coords: &mut [(i32, i32)],
    distance: impl Fn((i32, i32)) -> i32,
) {
    coords.sort_unstable_by_key(|&(x, y)| (distance((x, y)), y, x));
}

/// Convert an [`ImportedMesh`]'s parallel per-vertex arrays into renderer
/// [`Vertex`]es, applying the LOD fallback defaults (opaque white colour,
/// +Y normal, zero UV) for any array shorter than `positions` and copying
/// authored tangents where present.
///
/// The two LOD paths upload the result via `upload_scene_mesh_global_only`
/// (global-SSBO-only, no per-mesh buffers / no BLAS); the cell-loader's
/// `spawn_mesh_instance` (#2410) uploads the same vertices through the
/// RT-capable path instead. That difference is entirely in the *upload*
/// call — the `Vertex` values this builds are identical either way, which
/// is why the one builder serves all three.
pub(crate) fn imported_mesh_to_vertices(mesh: &ImportedMesh) -> Vec<Vertex> {
    (0..mesh.positions.len())
        .map(|i| {
            let color = mesh.colors.get(i).copied().unwrap_or([1.0, 1.0, 1.0, 1.0]);
            let normal = mesh.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
            let uv = mesh.uvs.get(i).copied().unwrap_or([0.0, 0.0]);
            let mut v = Vertex::new_rgba(mesh.positions[i], color, normal, uv);
            if let Some(t) = mesh.tangents.get(i) {
                v.tangent = *t;
            }
            v
        })
        .collect()
}

/// Local-space bounding sphere `(centre, radius)` of a mesh's positions:
/// the min/max AABB midpoint and the distance from it to the far corner.
///
/// Callers guard against empty position lists before calling — an empty
/// slice would leave the min/max sentinels un-updated and yield a NaN
/// centre, matching the pre-extraction inline behaviour.
pub(crate) fn local_aabb_center_radius(positions: &[[f32; 3]]) -> (Vec3, f32) {
    let mut lmin = Vec3::splat(f32::INFINITY);
    let mut lmax = Vec3::splat(f32::NEG_INFINITY);
    for p in positions {
        let v = Vec3::from_array(*p);
        lmin = lmin.min(v);
        lmax = lmax.max(v);
    }
    let centre = (lmin + lmax) * 0.5;
    let radius = (lmax - centre).length();
    (centre, radius)
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
    use byroredux_plugin::esm::cell::{CellData, WorldspaceRecord};
    use byroredux_plugin::esm::records::EsmIndex;
    use std::sync::Arc;
    use std::time::Duration;

    fn exterior_context(index: EsmIndex, worldspace_key: &str) -> ExteriorWorldContext {
        ExteriorWorldContext {
            record_index: Arc::new(index),
            load_order: Arc::new(Vec::new()),
            plugin_path: String::new(),
            plugin_paths: Vec::new(),
            worldspace_key: worldspace_key.to_string(),
            climate: None,
            default_weather: None,
            default_water_height: None,
            default_water_type_form: None,
        }
    }

    fn empty_cell(grid: (i32, i32)) -> CellData {
        CellData {
            form_id: 0,
            editor_id: String::new(),
            display_name: None,
            references: Vec::new(),
            is_interior: false,
            grid: Some(grid),
            lighting: None,
            landscape: None,
            water_height: None,
            water_height_is_explicit: false,
            image_space_form: None,
            water_type_form: None,
            acoustic_space_form: None,
            music_type_form: None,
            music_type_enum: None,
            climate_override: None,
            location_form: None,
            regions: Vec::new(),
            lighting_template_form: None,
            ownership: None,
            regional_color_override: None,
            precombined_mesh_hashes: Vec::new(),
            absorbed_refs: std::collections::HashSet::new(),
            navmeshes: Vec::new(),
        }
    }

    /// A mesh with only positions authored — every other per-vertex array
    /// empty, so `imported_mesh_to_vertices` must apply its fallbacks.
    fn positions_only(positions: Vec<[f32; 3]>) -> ImportedMesh {
        ImportedMesh::from_geometry(
            positions,
            Vec::new(), // colors
            Vec::new(), // normals
            Vec::new(), // tangents
            Vec::new(), // uvs
            Vec::new(), // indices
        )
    }

    /// #2452 / EXAL-04 — Skyrim and FO4 alone ship combined `.bto`/`.btr`.
    /// #3100 keeps that fact distinct from the older authored landscape-only
    /// LOD families so neither capability lies about the other.
    #[test]
    fn combined_and_legacy_lod_capabilities_are_disjoint() {
        assert!(combined_lod_supported(GameKind::Skyrim));
        assert!(combined_lod_supported(GameKind::Fallout4));
        assert!(!combined_lod_supported(GameKind::Oblivion));
        assert!(!combined_lod_supported(GameKind::Fallout3NV));
        assert!(!combined_lod_supported(GameKind::Fallout76));
        assert!(!combined_lod_supported(GameKind::Starfield));

        assert!(legacy_landscape_lod_supported(GameKind::Oblivion));
        assert!(legacy_landscape_lod_supported(GameKind::Fallout3NV));
        assert!(!legacy_landscape_lod_supported(GameKind::Skyrim));
        assert!(!legacy_landscape_lod_supported(GameKind::Fallout4));
        assert!(!legacy_landscape_lod_supported(GameKind::Fallout76));
        assert!(!legacy_landscape_lod_supported(GameKind::Starfield));
    }

    /// The combined-format predicate and band ladder answer the same question
    /// in two forms. Pinned so the two can't drift apart the way the inline
    /// `matches!` copies did.
    #[test]
    fn combined_lod_predicate_agrees_with_the_band_ladder() {
        use super::super::lod_bands::LodBandLadder;
        for game in [
            GameKind::Oblivion,
            GameKind::Fallout3NV,
            GameKind::Skyrim,
            GameKind::Fallout4,
            GameKind::Fallout76,
            GameKind::Starfield,
        ] {
            assert_eq!(
                combined_lod_supported(game),
                LodBandLadder::for_game(game).is_some(),
                "{game:?}: combined-LOD predicate and band ladder disagree"
            );
        }
    }

    #[test]
    fn vertices_apply_lod_fallback_defaults_when_arrays_short() {
        // Only positions authored — colour/normal/uv fall back, no tangent.
        let mesh = positions_only(vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
        let verts = imported_mesh_to_vertices(&mesh);
        assert_eq!(verts.len(), 2);
        assert_eq!(verts[0].position, [1.0, 2.0, 3.0]);
        // Vertex::new_rgba stores colour as rgba; the fallback is opaque white.
        assert_eq!(verts[0].color, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(verts[0].normal, [0.0, 1.0, 0.0]);
        assert_eq!(verts[0].uv, [0.0, 0.0]);
        assert_eq!(verts[0].tangent, [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn aabb_center_radius_is_midpoint_and_corner_distance() {
        // Unit cube corners → centre (0.5,0.5,0.5), radius = |(0.5,0.5,0.5)|.
        let positions = vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
        let (centre, radius) = local_aabb_center_radius(&positions);
        assert_eq!(centre, Vec3::splat(0.5));
        assert!((radius - Vec3::splat(0.5).length()).abs() < 1e-6);
    }

    /// EX-07 / #2376 — the wall-clock bound is what actually protects the
    /// frame. An attempt count cannot: since #2371 one attempt may be a
    /// level-4 quad or a level-32 macro-mesh covering 64× the ground, and the
    /// count cannot tell them apart.
    #[test]
    fn lod_work_budget_yields_once_the_shared_deadline_passes() {
        // An already-elapsed deadline still admits exactly one attempt, so a
        // frame whose full-detail apply consumed the whole allowance cannot
        // stall the ring forever.
        let mut budget = LodWorkBudget::with_deadline(8, Instant::now());
        assert!(budget.try_take(), "first attempt must always be admitted");
        assert!(
            !budget.try_take(),
            "past the deadline, the second attempt must yield to the next frame"
        );
        assert_eq!(budget.spent(), 1);

        // A live deadline lets the attempt cap remain the binding limit.
        let mut budget = LodWorkBudget::with_deadline(3, Instant::now() + Duration::from_secs(30));
        assert!(budget.try_take());
        assert!(budget.try_take());
        assert!(budget.try_take());
        assert!(
            !budget.try_take(),
            "attempt cap still applies under a deadline"
        );
        assert_eq!(budget.spent(), 3);
    }

    /// The deterministic full-radius bootstrap must stay untimed — it runs
    /// before the first frame is presented, where yielding would leave the
    /// initial ring half-built.
    #[test]
    fn unlimited_budget_has_no_deadline() {
        let mut budget = LodWorkBudget::unlimited();
        for _ in 0..1_000 {
            assert!(budget.try_take());
        }
    }

    #[test]
    fn lod_work_budget_caps_attempts_and_tracks_spend() {
        let mut budget = LodWorkBudget::new(2);
        assert!(budget.try_take());
        assert!(budget.try_take());
        assert!(!budget.try_take());
        assert_eq!(budget.spent(), 2);

        let mut unlimited = LodWorkBudget::unlimited();
        for _ in 0..1_000 {
            assert!(unlimited.try_take());
        }
        assert_eq!(unlimited.spent(), 1_000);
    }

    #[test]
    fn lod_coordinate_order_is_nearest_first_with_stable_ties() {
        let mut coords = vec![(2, 0), (0, 1), (-1, 0), (1, 0), (0, -1)];
        sort_lod_coords_nearest(&mut coords, |coord| coord.0.abs().max(coord.1.abs()));
        assert_eq!(coords, vec![(0, -1), (-1, 0), (1, 0), (0, 1), (2, 0)]);
    }

    #[test]
    fn quad_origin_respects_nonzero_worldspace_alignment() {
        assert_eq!(quad_origin(-49, -49, 4, (-50, -50)), (-50, -50));
        assert_eq!(quad_origin(-49, -48, 4, (-50, -50)), (-50, -50));
        assert_eq!(quad_origin(-48, -48, 4, (-50, -50)), (-50, -50));
        assert_eq!(quad_origin(-46, -46, 4, (-50, -50)), (-46, -46));

        assert_eq!(quad_origin(-50, -49, 4, (-52, -51)), (-52, -51));
        assert_eq!(quad_origin(-48, -47, 4, (-52, -51)), (-48, -47));
    }

    #[test]
    fn lod_grid_origin_prefers_wrld_min_cell() {
        let mut index = EsmIndex {
            game: GameKind::Skyrim,
            ..Default::default()
        };
        index.cells.worldspaces.insert(
            "dlc2apocryphaworld".into(),
            WorldspaceRecord {
                usable_min: (-50.0 * EXTERIOR_CELL_UNITS, -50.0 * EXTERIOR_CELL_UNITS),
                usable_max: (20.0 * EXTERIOR_CELL_UNITS, 20.0 * EXTERIOR_CELL_UNITS),
                ..Default::default()
            },
        );
        // A deliberately different explicit-cell minimum proves NAM0 wins.
        index
            .cells
            .exterior_cells
            .entry("dlc2apocryphaworld".into())
            .or_default()
            .insert((-60, -60), empty_cell((-60, -60)));
        let context = exterior_context(index, "dlc2apocryphaworld");

        assert_eq!(worldspace_lod_grid_origin(&context), (-50, -50));
    }

    #[test]
    fn lod_grid_origin_falls_back_to_explicit_cell_minimum() {
        let mut index = EsmIndex {
            game: GameKind::Skyrim,
            ..Default::default()
        };
        let cells = index
            .cells
            .exterior_cells
            .entry("dlc01soulcairn".into())
            .or_default();
        cells.insert((-48, -55), empty_cell((-48, -55)));
        cells.insert((-52, -51), empty_cell((-52, -51)));
        let context = exterior_context(index, "dlc01soulcairn");

        assert_eq!(worldspace_lod_grid_origin(&context), (-52, -55));
    }

    #[test]
    fn pre_skyrim_lod_keeps_absolute_zero_alignment() {
        let mut index = EsmIndex {
            game: GameKind::Oblivion,
            ..Default::default()
        };
        index.cells.worldspaces.insert(
            "tamriel".into(),
            WorldspaceRecord {
                usable_min: (-30.0 * EXTERIOR_CELL_UNITS, -50.0 * EXTERIOR_CELL_UNITS),
                usable_max: (30.0 * EXTERIOR_CELL_UNITS, 50.0 * EXTERIOR_CELL_UNITS),
                ..Default::default()
            },
        );
        let context = exterior_context(index, "tamriel");

        assert_eq!(worldspace_lod_grid_origin(&context), (0, 0));
    }
}
