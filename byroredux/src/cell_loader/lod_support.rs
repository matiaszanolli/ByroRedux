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

use std::collections::HashSet;
use std::time::Instant;

use byroredux_core::ecs::WorldBound;
use byroredux_core::math::Vec3;
use byroredux_nif::import::ImportedMesh;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_renderer::{Vertex, VulkanContext};

use crate::asset_provider::TextureProvider;

use super::LegacyLodQuadIndex;
use crate::env_translate::{terrain_lod_layout, TerrainLodLayout};

use super::exterior::ExteriorWorldContext;

/// Release GPU resources acquired by a distant-LOD spawn attempt or owned by
/// a resident block. Rollback and normal unload therefore share one ownership
/// contract instead of maintaining parallel drop sequences.
pub(crate) fn release_lod_gpu_resources(
    ctx: &mut VulkanContext,
    mesh_handles: &[u32],
    texture_handles: &[u32],
) {
    if let Some(accel) = ctx.accel_manager.as_mut() {
        for &handle in mesh_handles {
            accel.drop_blas(handle);
        }
    }
    ctx.mesh_registry.drop_meshes(mesh_handles);
    ctx.texture_registry
        .drop_textures(&ctx.device, texture_handles);
}

/// Immutable inputs shared by terrain and both game-specific object LOD
/// providers during one ring reconciliation.
pub(crate) struct LodReconcileInput<'a> {
    pub(crate) tex_provider: &'a TextureProvider,
    pub(crate) wctx: &'a ExteriorWorldContext,
    pub(crate) player_grid: (i32, i32),
    /// Worldspace-relative cell-grid origin used by Skyrim+/FO4 baked LOD.
    ///
    /// #5222 — the Fallout legacy family no longer consults this: its
    /// authored quads sit on per-worldspace lattices no single origin
    /// reproduces, so those rings select from the archive name-table index
    /// ([`LegacyLodQuadIndex`]) instead.
    pub(crate) lod_grid_origin: (i32, i32),
    /// #5222 — the authored-quad index for the Fallout legacy family
    /// (`None` on every other game; scanned once per streaming state).
    /// Object and terrain rings alike select their quads from it.
    pub(crate) legacy_lod_quads: Option<&'a LegacyLodQuadIndex>,
    /// Cells whose full-detail representation is actually resident now.
    /// The unload radius is only a possible residency bound, not a promise
    /// that every cell inside it has been populated.
    pub(crate) resident_full_cells: &'a HashSet<(i32, i32)>,
    /// Chebyshev radius (`radius_unload`) within which a full-detail cell may be
    /// resident. The terrain bands subdivide to the finest level wherever a
    /// coarse quad would reach it (#6094): their refine thresholds were tuned
    /// against a ~6-cell streaming radius, not the default 12. Consumers feed
    /// it through [`coarse_refine_radius`], which adds the one-cell margin
    /// (#6452).
    pub(crate) full_detail_radius: i32,
}

/// #6452 — cells of finest-band ground kept between a coarse LOD quad and the
/// hysteresis-resident ring. Only the finest band is holed against residency
/// and carries skirts / rim keys (#6409); a coarse quad (a skirtless `.btr`, or
/// a synth block whose rebuild key is `(0, 0)`) touching a cell at
/// `radius_unload` would bring back the sky-coloured ribbon #6409 fixed.
pub(crate) const COARSE_LOD_RESIDENT_MARGIN_CELLS: i32 = 1;

/// The Chebyshev radius within which a coarse terrain quad subdivides, for a
/// resident full-detail radius of `full_detail_radius` (`-1` disables, as for
/// `LodBandSelection::refine_within`).
pub(crate) const fn coarse_refine_radius(full_detail_radius: i32) -> i32 {
    if full_detail_radius < 0 {
        full_detail_radius
    } else {
        full_detail_radius + COARSE_LOD_RESIDENT_MARGIN_CELLS
    }
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

/// Archive key of a worldspace's authored LOD descriptor (#2586).
pub(crate) fn lod_settings_path(lod_worldspace_key: &str) -> String {
    format!("lodsettings\\{lod_worldspace_key}.lod")
}

/// The SW cell of an authored `lodsettings\<ws>.lod` descriptor — the cell
/// every baked `.btr` / `.bto` quad of that worldspace tiles from (#2586).
///
/// Vanilla Skyrim SE and Fallout 4 ship it as 16 bytes: `i16 x, i16 y`, then
/// three `u32`s (the extent in cells, the finest level, the coarsest level —
/// `tamriel.lod` is `-96 -96 | 256 4 32`). Only the cell is read: the rest
/// is not needed to place a quad and has no consumer. `None` when the file is
/// too short to hold the cell.
pub(crate) fn parse_lod_settings_origin(bytes: &[u8]) -> Option<(i32, i32)> {
    // Signed cells: `le_u16_at` is the sanctioned checked read, reinterpreted.
    let x = byroredux_core::bytes::le_u16_at(bytes, 0)? as i16;
    let y = byroredux_core::bytes::le_u16_at(bytes, 2)? as i16;
    Some((i32::from(x), i32::from(y)))
}

/// The baked-LOD grid origin the quad lattice really tiles from (#2586).
///
/// The authored `lodsettings\<ws>.lod` descriptor wins; `read_archive` maps an
/// archive key to the file's bytes. WRLD `NAM0` is only a stand-in
/// ([`worldspace_lod_grid_origin`]) and is wrong for the largest worldspace:
/// Tamriel's `NAM0` minimum is (-57, -43) while its quads sit on the `.lod`
/// cell (-96, -96), so every `tamriel.<L>.*.btr/.bto` probe missed. The
/// descriptor is read from the LOD source worldspace, the same key the quad
/// paths use ([`ExteriorWorldContext::lod_worldspace_key`]).
pub(crate) fn resolve_lod_grid_origin(
    wctx: &ExteriorWorldContext,
    read_archive: impl FnOnce(&str) -> Option<Vec<u8>>,
) -> (i32, i32) {
    if combined_lod_supported(wctx.record_index.game) {
        let path = lod_settings_path(wctx.lod_worldspace_key());
        if let Some(origin) = read_archive(&path)
            .as_deref()
            .and_then(parse_lod_settings_origin)
        {
            return origin;
        }
    }
    worldspace_lod_grid_origin(wctx)
}

/// Fallback baked-LOD grid origin for the active worldspace (#2586), used when
/// no `lodsettings\<ws>.lod` descriptor is readable
/// ([`resolve_lod_grid_origin`]).
///
/// Skyrim+/FO4 align `.bto` and `.btr` quad coordinates relative to the
/// worldspace's own SW cell, not global `(0, 0)`. That cell equals WRLD NAM0's
/// minimum for 11 of the 12 vanilla Skyrim worldspaces but not for Tamriel,
/// which is why the descriptor is read first. Prefer WRLD NAM0 via
/// `usable_cell_bounds`; derived/synthetic worldspaces that omit NAM0 fall
/// back to the component-wise minimum of their explicit exterior cells.
/// Older games keep their existing absolute synth/placement grids.
///
/// #5222 — the FO3/FNV legacy family no longer derives anything from this:
/// its authored quads sit on per-worldspace lattices (26 of 29
/// worldspace/level sets off `(0,0)`; no single origin — NAM0 included —
/// explains them, and `washmontop` level 8 alone spans three y residues),
/// so those rings select from the archive name-table index
/// (`legacy_lod_index::LegacyLodQuadIndex`) by footprint instead. The
/// `(0, 0)` early return below survives for the synth/placement grids that
/// still consume it.
///
/// #5496 — the quads were authored against the LOD source worldspace's
/// minimum cell (the PNAM "Use LOD Data" ancestor of an inheriting child),
/// so the origin resolves there too: the child's own bounds (a rooftop, a
/// walled city) would misalign every inherited quad.
pub(crate) fn worldspace_lod_grid_origin(wctx: &ExteriorWorldContext) -> (i32, i32) {
    if !combined_lod_supported(wctx.record_index.game) {
        return (0, 0);
    }

    let lod_key = wctx.lod_worldspace_key();
    let cells = &wctx.record_index.cells;
    if let Some(origin) = cells
        .worldspaces
        .get(lod_key)
        .and_then(|worldspace| worldspace.usable_cell_bounds())
        .map(|(min, _)| min)
    {
        return origin;
    }

    cells
        .exterior_cells
        .get(lod_key)
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
    // #5496 — the band bounds describe the distant ring, which is
    // authored on the LOD source worldspace: an inheriting child's own
    // cell extent (a rooftop, a walled city) would clip the parent's
    // quads out of every band.
    let lod_key = wctx.lod_worldspace_key();
    let cells = &wctx.record_index.cells;
    if let Some(bounds) = cells
        .worldspaces
        .get(lod_key)
        .and_then(|worldspace| worldspace.usable_cell_bounds())
    {
        return Some(bounds);
    }

    cells
        .exterior_cells
        .get(lod_key)
        .and_then(|world_cells| {
            world_cells.keys().copied().map(|c| (c, c)).reduce(|a, b| {
                (
                    (a.0 .0.min(b.0 .0), a.0 .1.min(b.0 .1)),
                    (a.1 .0.max(b.1 .0), a.1 .1.max(b.1 .1)),
                )
            })
        })
}

/// Whether the LOD source worldspace authors an exterior cell at `(x, y)`.
/// The coverage oracle that breaks `prune_same_level_overlaps`' count ties
/// (#5520); on the same worldspace as [`worldspace_cell_bounds`], so an
/// inheriting child reads its "Use LOD Data" ancestor's cells.
pub(crate) fn lod_source_cell_exists(wctx: &ExteriorWorldContext) -> impl Fn(i32, i32) -> bool + '_ {
    let cells = wctx
        .record_index
        .cells
        .exterior_cells
        .get(wctx.lod_worldspace_key());
    move |x, y| cells.is_some_and(|c| c.contains_key(&(x, y)))
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
    /// #6092 — candidates this ring left unresolved because the geometry pool
    /// refused admission (see [`LodAttemptVerdict::HeldBack`]).
    held_back: usize,
}

impl LodWorkBudget {
    /// Attempt-capped only — no time bound. For deterministic bootstrap and
    /// tests; the per-frame path uses [`Self::with_deadline`].
    pub(crate) fn new(max_attempts: usize) -> Self {
        Self {
            remaining: max_attempts,
            spent: 0,
            deadline: None,
            held_back: 0,
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
            held_back: 0,
        }
    }

    pub(crate) fn unlimited() -> Self {
        Self::new(usize::MAX)
    }

    /// Select the right constructor from a reconcile call's two independent
    /// inputs — an attempt cap (possibly `usize::MAX`) and an optional
    /// deadline — WITHOUT letting either one silently override the other.
    ///
    /// #3823 (REN-WD-D15-02) — extracted from `streaming_helpers::
    /// reconcile_lod_rings`'s `make_budget` closure so this selection logic
    /// is unit-testable on its own. The bug this guards against: an earlier
    /// version matched on `(usize::MAX, _)` first, so ANY deadline passed
    /// alongside an unbounded attempt count was silently discarded — the
    /// per-frame boundary-crossing caller in `app_step.rs` passes exactly
    /// that combination (unbounded attempts, a real deadline) as its safety
    /// ceiling against a pathological LOD ring, and that ceiling would have
    /// been a no-op. `unlimited()` (genuinely untimed) is reached only by
    /// the one-time world-setup bootstrap's `(usize::MAX, None)`.
    pub(crate) fn for_reconcile(max_attempts: usize, deadline: Option<Instant>) -> Self {
        match (max_attempts, deadline) {
            (usize::MAX, None) => Self::unlimited(),
            (attempts, Some(at)) => Self::with_deadline(attempts, at),
            (attempts, None) => Self::new(attempts),
        }
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

    /// #6092 — record `n` candidates the ring could not resolve because the
    /// geometry pool is at capacity. They are neither resident nor a known
    /// miss: the ring stops (nothing it can try now) and a later reconcile
    /// retries them once admission reopens.
    pub(crate) fn note_held_back(&mut self, n: usize) {
        self.held_back = self.held_back.saturating_add(n);
    }

    /// Candidates held back by a closed geometry pool this reconcile.
    pub(crate) fn held_back(self) -> usize {
        self.held_back
    }
}

/// What a distant-LOD attempt that produced no block means (#6092).
///
/// Every ring used to read `None` as "this quad has no asset" and cache that:
/// terrain in `missing_blocks`, the others as an empty sentinel. But
/// `upload_scene_mesh_global_only` also returns `Err` once the pool hits its
/// hard cap, and that refusal says nothing about the asset. The pool's
/// admission flag is latched closed until a compaction reopens it, so it is the
/// discriminator: it was open when the attempt began (every ring checks), so
/// finding it closed afterwards means the attempt tripped it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LodAttemptVerdict {
    /// The attempt produced a block: keep it.
    Keep,
    /// No asset, or an unusable one, with the pool still open: a real miss,
    /// safe to remember so the ring does not re-extract it every crossing.
    Miss,
    /// The pool refused the upload. Remember nothing; the quad is retried when
    /// admission reopens.
    HeldBack,
}

/// Classify a finished attempt from whether it produced a block and whether
/// the pool still admits uploads *after* it ran.
pub(crate) fn classify_lod_attempt(produced: bool, admission_open_after: bool) -> LodAttemptVerdict {
    match (produced, admission_open_after) {
        (true, _) => LodAttemptVerdict::Keep,
        (false, true) => LodAttemptVerdict::Miss,
        (false, false) => LodAttemptVerdict::HeldBack,
    }
}

/// Whether a starved LOD reconcile should be armed again (#6092).
///
/// A `Starved` verdict stops the reconcile because retrying against a closed
/// pool only re-decodes and re-rejects. The pool reopens at the next
/// compaction after a cell unload, and nothing else tells the rings, so this is
/// the edge that restarts them. One level-triggered flag is enough: it is set
/// by the starved arm and cleared by the first frame that finds the pool open.
pub(crate) fn lod_should_rearm_after_starvation(starved: bool, admission_open: bool) -> bool {
    starved && admission_open
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

/// #5820 — the "drop what left the ring, count unloads" step every
/// streaming-ring reconciler runs between computing its desired set and
/// spending its spawn budget. `unload` frees one evicted block and
/// returns whether it freed anything: the object/placement/tree rings
/// return `false` for an empty sentinel (nothing to free, but the block
/// still leaves the map); terrain has no sentinels and always returns
/// `true`. Keeping that policy inside the closure is what lets one body
/// serve all four rings — the empty-sentinel rule stays per-ring, while
/// the retain shape itself (and the leak-fix class of #863/#4986, a
/// change to how eviction frees GPU resources) lands in one place.
pub(crate) fn retain_resident<K, B>(
    blocks: &mut std::collections::HashMap<K, B>,
    desired: &HashSet<K>,
    mut unload: impl FnMut(&mut B) -> bool,
) -> usize
where
    K: std::hash::Hash + Eq,
{
    let mut unloaded = 0usize;
    blocks.retain(|coord, blk| {
        if desired.contains(coord) {
            true
        } else {
            if unload(blk) {
                unloaded += 1;
            }
            false
        }
    });
    unloaded
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

/// World-space bounding sphere over already-world-space `vertices`, ignoring
/// every index for which `skip` is true (the terrain path skips hole vertices;
/// the `.btr` path skips none). Falls back to [`WorldBound::ZERO`] when no
/// vertex survives (the callers have already rejected empty / all-hole
/// meshes, so this is a degenerate-input guard, not a normal result).
pub(crate) fn vertex_bound(vertices: &[Vertex], skip: impl Fn(usize) -> bool) -> WorldBound {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for (i, v) in vertices.iter().enumerate() {
        if skip(i) {
            continue;
        }
        let p = Vec3::from_array(v.position);
        min = min.min(p);
        max = max.max(p);
    }
    if !min.is_finite() {
        return WorldBound::ZERO;
    }
    let center = (min + max) * 0.5;
    let radius = (max - center).length();
    WorldBound::new(center, radius)
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
    use byroredux_plugin::esm::cell::{CellData, WorldspaceRecord};
    use byroredux_plugin::esm::records::EsmIndex;
    use std::sync::Arc;
    use std::time::Duration;

    /// #5820 — the shared drop-what-left-the-ring step: keeps desired
    /// keys, evicts the rest, counts only closures that freed something,
    /// and still drops a non-freeing (empty-sentinel) block.
    #[test]
    fn retain_resident_evicts_counts_and_honours_sentinels() {
        let mut blocks: std::collections::HashMap<(i32, i32), Vec<u8>> = [
            ((0, 0), vec![1u8]),
            ((1, 0), vec![2u8]),
            ((2, 0), Vec::new()), // empty sentinel: dropped, not counted
        ]
        .into_iter()
        .collect();
        let desired: HashSet<(i32, i32)> = [(0, 0)].into_iter().collect();
        let unloaded = retain_resident(&mut blocks, &desired, |blk| {
            let had = !blk.is_empty();
            blk.clear();
            had
        });
        assert_eq!(blocks.keys().copied().collect::<Vec<_>>(), vec![(0, 0)]);
        assert_eq!(unloaded, 1, "only the non-empty eviction counts");
    }

    /// #5820 — every streaming-ring reconciler routes its eviction
    /// through [`retain_resident`], so the empty-sentinel rule and any
    /// future eviction-policy change stay per-ring while the retain
    /// shape has one home. A fifth ring re-growing the inline
    /// `blocks.retain(|coord, blk| { … desired … unload … false })`
    /// loop is the duplication class this closed four copies of.
    #[test]
    fn every_lod_ring_routes_eviction_through_retain_resident() {
        for ring in ["object_lod", "placement_lod", "terrain_lod", "tree_lod"] {
            let path = format!(
                "{}/src/cell_loader/{ring}.rs",
                env!("CARGO_MANIFEST_DIR")
            );
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
            assert!(
                source.matches("retain_resident(").count() >= 1,
                "{ring} must evict through lod_support::retain_resident (#5820)"
            );
            assert!(
                !source.contains(".retain(|coord, blk|"),
                "{ring} re-grew an inline eviction retain loop (#5820)"
            );
        }
    }

    fn exterior_context(index: EsmIndex, worldspace_key: &str) -> ExteriorWorldContext {
        ExteriorWorldContext {
            record_index: Arc::new(index),
            load_order: Arc::new(Default::default()),
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
            grid: Some(grid),
            ..Default::default()
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

    /// #3823 (REN-WD-D15-02) — `for_reconcile` must not let an unbounded
    /// attempt count silently discard a caller-supplied deadline. This is
    /// the exact combination `app_step.rs`'s boundary-crossing safety
    /// ceiling relies on: unbounded attempts (every newly-exposed LOD
    /// footprint should settle atomically) PAIRED with a deadline (so a
    /// pathological ring cannot hang the frame).
    #[test]
    fn for_reconcile_honors_a_deadline_even_with_unbounded_attempts() {
        let mut budget = LodWorkBudget::for_reconcile(usize::MAX, Some(Instant::now()));
        assert!(
            budget.try_take(),
            "first attempt must always be admitted regardless of the deadline"
        );
        assert!(
            !budget.try_take(),
            "an already-elapsed deadline must still yield the second attempt, \
             even though the attempt count is unbounded — this is the safety \
             ceiling a pathological LOD ring relies on"
        );
    }

    /// The genuinely-untimed bootstrap combination must still route to
    /// `unlimited()`, not accidentally pick up a deadline from nowhere.
    #[test]
    fn for_reconcile_stays_untimed_with_no_deadline() {
        let mut budget = LodWorkBudget::for_reconcile(usize::MAX, None);
        for _ in 0..1_000 {
            assert!(budget.try_take());
        }
    }

    /// A finite attempt count with a live deadline still caps on attempts
    /// first, matching `with_deadline`'s own contract — `for_reconcile` must
    /// not special-case away the ordinary bounded-and-timed path either.
    #[test]
    fn for_reconcile_finite_attempts_still_caps_under_a_live_deadline() {
        let mut budget =
            LodWorkBudget::for_reconcile(2, Some(Instant::now() + Duration::from_secs(30)));
        assert!(budget.try_take());
        assert!(budget.try_take());
        assert!(!budget.try_take(), "attempt cap must still apply");
    }

    /// #6092 — a refused upload is not a miss. A pool-refused attempt must never
    /// be remembered (the old rings cached it for good), and a real miss with the
    /// pool open still is.
    #[test]
    fn a_capacity_refusal_is_held_back_not_cached_as_a_miss() {
        use LodAttemptVerdict::*;
        assert_eq!(classify_lod_attempt(true, true), Keep);
        // A block that fit and then closed the pool is still a block.
        assert_eq!(classify_lod_attempt(true, false), Keep);
        assert_eq!(classify_lod_attempt(false, true), Miss);
        assert_eq!(classify_lod_attempt(false, false), HeldBack);
    }

    /// #6332 — the rings that upload several sub-meshes per quad / cell take a refused group's
    /// uploads back out of the pool instead of dropping them: the dropped spans become a hole, the
    /// next geometry rebuild compacts and reopens admission, and the same group is re-attempted
    /// against the same free space. The registry half is `abandoned_group_tests`; this pins the
    /// two rings that need it (terrain and tree LOD upload one mesh per attempt and leave no
    /// partial).
    #[test]
    fn multi_mesh_lod_rings_abandon_a_refused_group_instead_of_dropping_it() {
        for (ring, src) in [
            ("object_lod.rs", include_str!("object_lod.rs")),
            ("placement_lod.rs", include_str!("placement_lod.rs")),
        ] {
            let src = byroredux_core::source_scan::production_text(src);
            assert!(
                src.contains("ctx.mesh_registry.begin_scene_geometry_group()"),
                "{ring} no longer marks the start of its sub-mesh group (#6332)"
            );
            assert!(
                src.contains("ctx.mesh_registry.abandon_scene_geometry_group(group)"),
                "{ring} no longer takes a refused group back out of the pool (#6332)"
            );
        }
    }

    /// #6092 — two reconciles against a fake admission state: the first, with
    /// the pool closed, holds the quad back and records nothing; the second,
    /// after the pool reopens, finds it still a candidate and resolves it.
    #[test]
    fn a_held_back_quad_is_retried_after_admission_reopens() {
        use std::collections::HashMap;
        let key = (4, 0, 0);
        // One reconcile over a single quad: skipped when cached as a known miss,
        // otherwise attempted, with a miss remembered exactly as the rings do.
        fn reconcile(
            missing: &mut HashMap<(i32, i32, i32), u16>,
            key: (i32, i32, i32),
            admission_open: bool,
            asset_exists: bool,
        ) -> Option<LodAttemptVerdict> {
            if missing.contains_key(&key) {
                return None;
            }
            let verdict = classify_lod_attempt(asset_exists && admission_open, admission_open);
            if verdict == LodAttemptVerdict::Miss {
                missing.insert(key, 0);
            }
            Some(verdict)
        }
        let mut missing = HashMap::new();
        assert_eq!(
            reconcile(&mut missing, key, false, true),
            Some(LodAttemptVerdict::HeldBack)
        );
        assert!(missing.is_empty(), "a refused attempt must leave no cache entry");
        assert_eq!(
            reconcile(&mut missing, key, true, true),
            Some(LodAttemptVerdict::Keep),
            "the retry must run, not be skipped as a known miss"
        );
        // A genuine miss with the pool open is still remembered and skipped.
        assert_eq!(
            reconcile(&mut missing, (4, 4, 0), true, false),
            Some(LodAttemptVerdict::Miss)
        );
        assert_eq!(reconcile(&mut missing, (4, 4, 0), true, false), None);
    }

    #[test]
    fn held_back_candidates_accumulate_on_the_budget() {
        let mut budget = LodWorkBudget::new(8);
        assert_eq!(budget.held_back(), 0);
        budget.note_held_back(3);
        budget.note_held_back(2);
        assert_eq!(budget.held_back(), 5);
        assert_eq!(budget.spent(), 0, "held-back candidates are not attempts");
    }

    /// #6092 — the starved arm re-arms the reconcile on the closed→open edge,
    /// and only then.
    #[test]
    fn starvation_rearms_only_once_admission_reopens() {
        assert!(!lod_should_rearm_after_starvation(true, false), "still closed");
        assert!(lod_should_rearm_after_starvation(true, true), "reopened");
        assert!(!lod_should_rearm_after_starvation(false, true), "never starved");
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

    /// The vanilla descriptors, byte for byte (`lodsettings\*.lod`, Skyrim SE
    /// `Skyrim - Meshes0.bsa`): `tamriel.lod`, `dlc2apocryphaworld.lod`,
    /// `markarthworld.lod`.
    const TAMRIEL_LOD: [u8; 16] = [
        0xa0, 0xff, 0xa0, 0xff, 0x00, 0x01, 0, 0, 0x04, 0, 0, 0, 0x20, 0, 0, 0,
    ];
    const APOCRYPHA_LOD: [u8; 16] = [
        0xce, 0xff, 0xce, 0xff, 0x80, 0x00, 0, 0, 0x04, 0, 0, 0, 0x20, 0, 0, 0,
    ];
    const MARKARTH_LOD: [u8; 16] = [
        0xd0, 0xff, 0xfd, 0xff, 0x20, 0x00, 0, 0, 0x04, 0, 0, 0, 0x20, 0, 0, 0,
    ];

    #[test]
    fn lod_settings_descriptor_gives_the_sw_cell() {
        assert_eq!(parse_lod_settings_origin(&TAMRIEL_LOD), Some((-96, -96)));
        assert_eq!(parse_lod_settings_origin(&APOCRYPHA_LOD), Some((-50, -50)));
        assert_eq!(parse_lod_settings_origin(&MARKARTH_LOD), Some((-48, -3)));
        assert_eq!(parse_lod_settings_origin(&TAMRIEL_LOD[..3]), None, "too short for the cell");
        assert_eq!(parse_lod_settings_origin(&[]), None);
        assert_eq!(lod_settings_path("tamriel"), "lodsettings\\tamriel.lod");
    }

    /// A Skyrim index whose Tamriel `NAM0` minimum is the real (-57, -43).
    fn tamriel_nam0_index() -> EsmIndex {
        let mut index = EsmIndex {
            game: GameKind::Skyrim,
            ..Default::default()
        };
        index.cells.worldspaces.insert(
            "tamriel".into(),
            WorldspaceRecord {
                usable_min: (-57.0 * EXTERIOR_CELL_UNITS, -43.0 * EXTERIOR_CELL_UNITS),
                usable_max: (61.0 * EXTERIOR_CELL_UNITS, 50.0 * EXTERIOR_CELL_UNITS),
                ..Default::default()
            },
        );
        index
    }

    /// #2586 (reopened) — Tamriel's quads tile from the `.lod` cell, not from
    /// `NAM0`: with (-57, -43) the lattice requests `tamriel.32.-57.-43` and
    /// every probe misses. Every real `tamriel.<L>.<x>.<y>` name has
    /// `x, y == 0 (mod L)`, so the origin must too.
    #[test]
    fn tamriel_origin_comes_from_the_lod_descriptor_not_nam0() {
        let context = exterior_context(tamriel_nam0_index(), "tamriel");
        assert_eq!(worldspace_lod_grid_origin(&context), (-57, -43), "the NAM0 stand-in");

        let mut asked = Vec::new();
        let origin = resolve_lod_grid_origin(&context, |path| {
            asked.push(path.to_owned());
            Some(TAMRIEL_LOD.to_vec())
        });
        assert_eq!(origin, (-96, -96));
        assert_eq!(asked, ["lodsettings\\tamriel.lod"]);
        for level in [4, 8, 16, 32] {
            assert_eq!(
                quad_origin(0, 0, level, origin),
                (0, 0),
                "level {level}: tamriel.{level}.0.0 is on the lattice"
            );
            assert_eq!(quad_origin(-96, -96, level, origin), (-96, -96));
        }
    }

    #[test]
    fn missing_lod_descriptor_falls_back_to_nam0() {
        let context = exterior_context(tamriel_nam0_index(), "tamriel");
        assert_eq!(resolve_lod_grid_origin(&context, |_| None), (-57, -43));
        assert_eq!(
            resolve_lod_grid_origin(&context, |_| Some(vec![0xa0])),
            (-57, -43),
            "a truncated descriptor is not trusted"
        );
    }

    /// An LOD-inheriting child reads its ancestor's descriptor (#5496): the
    /// child ships no `lodsettings` file of its own.
    #[test]
    fn an_inheriting_child_reads_the_ancestors_lod_descriptor() {
        let context = exterior_context(inheriting_child_index(), "whiterunworld");
        let mut asked = String::new();
        let origin = resolve_lod_grid_origin(&context, |path| {
            asked = path.to_owned();
            Some(TAMRIEL_LOD.to_vec())
        });
        assert_eq!(origin, (-96, -96));
        assert_eq!(asked, "lodsettings\\tamriel.lod");
    }

    /// Games without the combined `.btr`/`.bto` family never touch the
    /// archive: their grids are absolute.
    #[test]
    fn pre_skyrim_lod_never_reads_a_descriptor() {
        let index = EsmIndex {
            game: GameKind::Oblivion,
            ..Default::default()
        };
        let context = exterior_context(index, "tamriel");
        let origin = resolve_lod_grid_origin(&context, |path| panic!("read {path}"));
        assert_eq!(origin, (0, 0));
    }

    /// #2586 (reopened) on the real corpus: the descriptor's SW cell tiles
    /// every shipped `.btr` / `.bto` name of Tamriel (3,777 files, none of
    /// which the `NAM0` stand-in reaches) and of the DLC worlds it was
    /// first measured on. Markarth is left out on purpose: its name table
    /// mixes two generations of quads (`x, y == 0 (mod L)` leftovers beside
    /// the descriptor's lattice), so no single origin tiles all of it.
    ///
    ///   cargo test -p byroredux --bin byroredux lod_descriptor_tiles -- --ignored
    #[test]
    #[ignore = "needs Skyrim SE game data on disk"]
    fn lod_descriptor_tiles_the_shipped_quad_names() {
        let data = std::env::var("BYROREDUX_SKYRIMSE_DATA")
            .ok()
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data".into()
            });
        let (meshes0, meshes1) = (data.join("Skyrim - Meshes0.bsa"), data.join("Skyrim - Meshes1.bsa"));
        if !meshes0.is_file() || !meshes1.is_file() {
            assert!(
                std::env::var("BYROREDUX_REQUIRE_GAME_DATA").map_or(true, |v| v == "0"),
                "BYROREDUX_REQUIRE_GAME_DATA is set, but {meshes0:?} / {meshes1:?} is missing"
            );
            eprintln!("skipping: Skyrim meshes archives not found under {data:?}");
            return;
        }
        let descriptors = byroredux_bsa::BsaArchive::open(&meshes0).expect("open Skyrim - Meshes0.bsa");
        let origin_of = |world: &str| -> (i32, i32) {
            let bytes = descriptors
                .extract(&lod_settings_path(world))
                .unwrap_or_else(|e| panic!("{world}: {e}"));
            parse_lod_settings_origin(&bytes).expect("a 16-byte descriptor")
        };
        let quads = byroredux_bsa::BsaArchive::open(&meshes1).expect("open Skyrim - Meshes1.bsa");
        let names: Vec<String> = quads.list_files().into_iter().map(str::to_ascii_lowercase).collect();

        assert_eq!(origin_of("tamriel"), (-96, -96));
        for (world, expect_origin) in [
            ("tamriel", (-96, -96)),
            ("dlc2apocryphaworld", (-50, -50)),
            ("dlc01soulcairn", (-52, -51)),
            ("dlc2solstheimworld", (-64, -64)),
        ] {
            let origin = origin_of(world);
            assert_eq!(origin, expect_origin, "{world}");
            let prefix = format!("meshes\\terrain\\{world}\\");
            let mut tiled = 0;
            for name in names.iter().filter(|n| n.starts_with(&prefix)) {
                let stem = name.rsplit('\\').next().unwrap();
                let Some(coords) = stem
                    .strip_suffix(".btr")
                    .or_else(|| stem.strip_suffix(".bto"))
                    .and_then(|s| s.strip_prefix(world))
                else {
                    continue;
                };
                let mut parts = coords.trim_start_matches('.').split('.').map(str::parse::<i32>);
                let (Some(Ok(level)), Some(Ok(x)), Some(Ok(y))) = (parts.next(), parts.next(), parts.next())
                else {
                    continue;
                };
                assert_eq!(
                    quad_origin(x, y, level, origin),
                    (x, y),
                    "{name}: off the {world} lattice rooted at {origin:?}"
                );
                tiled += 1;
            }
            assert!(tiled > 100, "{world}: only {tiled} quad names found");
        }
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

    /// A Skyrim index with an LOD-authoring parent (`tamriel`, usable
    /// bounds from cell -50) and an LOD-inheriting child (`whiterunworld`,
    /// `PNAM` bit `0x02`, its own bounds from cell -2): the shape of every
    /// walled hold city (#5496).
    fn inheriting_child_index() -> EsmIndex {
        let mut index = EsmIndex {
            game: GameKind::Skyrim,
            ..Default::default()
        };
        index.cells.worldspaces.insert(
            "tamriel".into(),
            WorldspaceRecord {
                form_id: 0x3C,
                usable_min: (-50.0 * EXTERIOR_CELL_UNITS, -50.0 * EXTERIOR_CELL_UNITS),
                usable_max: (50.0 * EXTERIOR_CELL_UNITS, 50.0 * EXTERIOR_CELL_UNITS),
                ..Default::default()
            },
        );
        index.cells.worldspaces.insert(
            "whiterunworld".into(),
            WorldspaceRecord {
                form_id: 0x1A26F,
                parent_worldspace: Some(0x3C),
                parent_flags: 0x02, // pnam::INHERIT_LOD
                usable_min: (-2.0 * EXTERIOR_CELL_UNITS, -2.0 * EXTERIOR_CELL_UNITS),
                usable_max: (2.0 * EXTERIOR_CELL_UNITS, 2.0 * EXTERIOR_CELL_UNITS),
                ..Default::default()
            },
        );
        index
    }

    /// #5496 (reopened) — the quads of an LOD-inheriting child are the
    /// ancestor's, authored against the ancestor's minimum cell. The grid
    /// origin used the child's own bounds, shifting every inherited quad.
    #[test]
    fn lod_grid_origin_of_an_inheriting_child_is_the_ancestors() {
        let context = exterior_context(inheriting_child_index(), "whiterunworld");
        assert_eq!(worldspace_lod_grid_origin(&context), (-50, -50));

        // The same child without the bit keeps its own bounds.
        let mut index = inheriting_child_index();
        index.cells.worldspaces.get_mut("whiterunworld").unwrap().parent_flags = 0;
        let context = exterior_context(index, "whiterunworld");
        assert_eq!(worldspace_lod_grid_origin(&context), (-2, -2));
    }

    /// #5496 (reopened) — selection finds the ancestor's quads, so the
    /// archive and atlas paths must name the ancestor too: keyed on the
    /// child, the extract found nothing and the ring stayed empty.
    #[test]
    fn object_lod_paths_of_an_inheriting_child_name_the_ancestor() {
        use crate::cell_loader::object_lod::{object_lod_quad_paths, ObjectLodScheme};
        let context = exterior_context(inheriting_child_index(), "whiterunworld");
        let (archive, atlas) = object_lod_quad_paths(&context, ObjectLodScheme::BakedBto, 4, 0, -8);
        assert_eq!(archive, "meshes\\terrain\\tamriel\\objects\\tamriel.4.0.-8.bto");
        assert!(atlas.contains("tamriel"), "atlas path keyed on the ancestor: {atlas}");
        assert!(!atlas.contains("whiterun"), "atlas path must not name the child: {atlas}");
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

    #[test]
    fn empty_lod_spawns_release_acquired_gpu_resources_before_returning() {
        // #6092 — the rollback now also covers a quad the pool refused part of,
        // and releases through the block's own unload so a half-uploaded quad
        // leaves nothing resident. The unload (checked below) is what frees the
        // GPU resources, so the branch must route through it.
        for (name, src, rollback) in [
            ("object_lod.rs", include_str!("object_lod.rs"), "unload_object_lod_block("),
            ("placement_lod.rs", include_str!("placement_lod.rs"), "unload_placement_lod_block("),
        ] {
            let empty_branch = src
                .split("if refused || block.entities.is_empty() {")
                .nth(1)
                .and_then(|rest| rest.split("return None;").next())
                .unwrap_or_else(|| panic!("{name}: expected empty-entity rollback branch"));
            assert!(
                empty_branch.contains(rollback),
                "{name}: acquired mesh/texture handles must be released (via {rollback}) before an empty or refused spawn returns"
            );

            let unload = src
                .split("fn unload_")
                .nth(1)
                .unwrap_or_else(|| panic!("{name}: expected unload function"));
            assert!(
                unload.contains("release_lod_gpu_resources"),
                "{name}: rollback and normal unload must retain the same release contract"
            );
        }
    }

    fn bound_vertex(x: f32, y: f32, z: f32) -> Vertex {
        Vertex::new([x, y, z], [1.0; 3], [0.0, 1.0, 0.0], [0.0; 2])
    }

    #[test]
    fn vertex_bound_spans_vertices() {
        let verts = vec![
            bound_vertex(0.0, 0.0, 0.0),
            bound_vertex(2.0, 0.0, 0.0),
            bound_vertex(0.0, 0.0, 2.0),
        ];
        let b = vertex_bound(&verts, |_| false);
        assert_eq!(b.center, Vec3::new(1.0, 0.0, 1.0));
        // Half-diagonal of the [0,2]x{0}x[0,2] box from its centre.
        assert!((b.radius - 2f32.sqrt()).abs() < 1e-4);
    }

    #[test]
    fn vertex_bound_empty_is_zero() {
        assert_eq!(vertex_bound(&[], |_| false).center, WorldBound::ZERO.center);
    }

    /// The terrain path's hole mask: a skipped vertex must not stretch the
    /// sphere, and an all-skipped set degrades to ZERO.
    #[test]
    fn vertex_bound_ignores_skipped_vertices() {
        let verts = vec![
            bound_vertex(0.0, 0.0, 0.0),
            bound_vertex(2.0, 0.0, 0.0),
            bound_vertex(500.0, 0.0, 500.0),
        ];
        let holes = [false, false, true];
        let b = vertex_bound(&verts, |i| holes[i]);
        assert_eq!(b.center, Vec3::new(1.0, 0.0, 0.0));
        assert!((b.radius - 1.0).abs() < 1e-4);
        assert_eq!(vertex_bound(&verts, |_| true).center, WorldBound::ZERO.center);
    }
}
