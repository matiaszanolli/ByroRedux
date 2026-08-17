//! Distance-banded baked-LOD level selection — the 4/8/16/32 quad ladder
//! shared by distant terrain (`.btr`, [`super::terrain_lod`]) and distant
//! objects (`.bto`, [`super::object_lod`]). EX-11 / #2371.
//!
//! ## Why a quadtree and not four independent rings
//!
//! Skyrim+/FO4 bake their distant LOD as an exact quadtree: measured on
//! vanilla data (2026-08-12), `Skyrim - Meshes1.bsa` ships **2304 / 576 /
//! 144 / 36** `tamriel.<level>.*.btr` at levels 4 / 8 / 16 / 32 — a clean
//! 4:1 ratio at every step — and `Fallout4 - Meshes.ba2` ships exactly the
//! same counts for `commonwealth`. Four independent per-level rings would
//! have to be trimmed against each other to avoid double-drawing the same
//! ground; a single top-down descent instead **partitions** the world: each
//! node either emits itself or recurses into its four children, never both.
//! That makes "no overlap" and "no gaps" structural rather than a tuning
//! exercise (see the `partition_*` tests).
//!
//! ## Where the band distances come from
//!
//! The refine thresholds are the games' own `[TerrainManager]` block
//! distances, read from the shipped INIs rather than invented:
//!
//! | Setting | Skyrim SE `Ultra.ini` | FO4 `Ultra.ini` |
//! |---|---|---|
//! | `fBlockLevel0Distance` | 60000 | 60000 |
//! | `fBlockLevel1Distance` | 90000 | 90000 |
//! | `fBlockLevel2Distance` | *(absent)* | 110000 |
//! | `fBlockMaximumDistance` | 250000 | 250000 |
//!
//! `fBlockLevel<i>Distance` is the distance beyond which level-`LOD_LEVELS[i]`
//! quads are replaced by the next coarser level — i.e. it is the *refine*
//! threshold of level `LOD_LEVELS[i + 1]`. Skyrim authoring **no**
//! `fBlockLevel2Distance` is not an oversight: it is why Skyrim's coarsest
//! runtime band is 16, and it lines up exactly with the archive inventory —
//! Skyrim ships level-4/8/16 `.bto` object LOD and **no level-32 `.bto`**,
//! while FO4, which does author `fBlockLevel2Distance`, ships all four.
//! [`LodBandLadder::coarsest_level`] derives that from the ladder instead of
//! hardcoding a per-game level cap.
//!
//! `fSplitDistanceMult` (1.5 at Ultra) is deliberately **not** modelled: its
//! exact application point in the vanilla terrain manager is unverified, and
//! guessing it would move every band boundary.
//!
//! ## Two distance metrics, deliberately
//!
//! The vanilla settings are radial distances in BU; this module works in
//! cells, converted once at construction ([`cells_from_bu`]), and in
//! Chebyshev rather than Euclidean — a square approximation of a round
//! threshold, so a quad on the diagonal switches band slightly later than
//! vanilla would. That keeps one shape across the whole streaming stack
//! (cell ring, object ring, hole masks).
//!
//! Which *point* on the quad the distance is measured to differs by purpose,
//! and the split is load-bearing:
//!
//! * **Band selection** measures to the quad's **centre**
//!   ([`quad_center_chebyshev_halves`]). Band widths must be commensurate
//!   with quad sizes or a band is skipped outright, and the nearest-cell
//!   metric is not: FO4's level-16 window is `(22, 27]` cells wide — five
//!   cells — while level-16 quads step in units of 16, so *no* level-16 quad
//!   can ever land in it. Measured against real data that is self-evidently
//!   wrong: Skyrim ships 144 level-16 `.btr` and 48 level-16 `.bto`, and
//!   FO4 the same terrain count, which a nearest-cell ladder would never
//!   load. Centre distance steps in units of `level / 2` about a `level / 2`
//!   offset, so every authored band is reachable.
//! * **Ring extent and the full-detail exclusion** measure to the quad's
//!   **nearest cell** ([`quad_min_chebyshev`]). Those are containment
//!   questions, not detail-selection ones: a quad with any cell inside the
//!   streaming region must not draw over it (#1866 / #1871), and a quad with
//!   any cell inside the ring should still be drawn.
//!
//! Vanilla's exact measurement point is unverified — the engine source is
//! not available and the INI only gives the thresholds. Centre distance is
//! the reading that makes the shipped per-level asset counts consistent with
//! the shipped band distances; pinning it exactly is a job for the
//! cross-game capture calibration that EX-11 still owes.

use byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
use byroredux_plugin::esm::reader::GameKind;

use super::lod_support::quad_origin;

/// Canonical baked-LOD quad levels (cells per quad edge), finest first.
/// Level 4 is the highest-detail band; 32 is the coarsest and also the
/// tier the games render the world map from.
pub(crate) const LOD_LEVELS: [i32; 4] = [4, 8, 16, 32];

/// Band-switch hysteresis in cells. A quad already drawn at its own level
/// must come a full cell *closer* than the nominal threshold before it
/// subdivides, and a subdivided quad must retreat a full cell *past* it
/// before it merges back — so a player pacing across a boundary does not
/// thrash a whole band's worth of archive loads every step.
///
/// One cell matches the streaming layer's existing hysteresis convention
/// (`radius_unload == radius_load + 1`, `streaming.rs`) rather than
/// introducing a second, unrelated margin.
pub(crate) const LOD_BAND_HYSTERESIS_CELLS: i32 = 1;

/// Convert a vanilla `[TerrainManager]` BU distance into whole cells,
/// rounding to nearest. 60000 BU / 4096 = 14.65 → 15 cells.
fn cells_from_bu(bu: f32) -> i32 {
    (bu / EXTERIOR_CELL_UNITS).round() as i32
}

/// Vanilla `fBlockLevel{0,1,2}Distance` in BU, in ladder order, plus
/// `fBlockMaximumDistance`. Skyrim's ladder is one band shorter — it
/// authors no `fBlockLevel2Distance`.
const SKYRIM_ULTRA_REFINE_BU: &[f32] = &[60_000.0, 90_000.0];
const FALLOUT4_ULTRA_REFINE_BU: &[f32] = &[60_000.0, 90_000.0, 110_000.0];
const ULTRA_MAX_DISTANCE_BU: f32 = 250_000.0;

/// The per-game distance ladder that drives [`select_lod_quads`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LodBandLadder {
    /// `refine_cells[i]` is `fBlockLevel<i>Distance` in cells: the distance
    /// below which a level-`LOD_LEVELS[i + 1]` quad subdivides into four
    /// level-`LOD_LEVELS[i]` quads. Its length is the number of live band
    /// boundaries, which is what caps [`Self::coarsest_level`].
    refine_cells: Vec<i32>,
    /// `fBlockMaximumDistance` in cells — the outer extent of the ring.
    max_cells: i32,
}

impl LodBandLadder {
    /// The ladder for `game`, or `None` for titles that ship no baked
    /// quadtree LOD at all (Oblivion / FO3 / FNV). Those keep their existing
    /// single-band schemes — the synthesized heightmap ring and, for
    /// Oblivion, `DistantLOD\*.lod` + `_far.nif` placement — which have no
    /// coarser source to select between.
    pub(crate) fn for_game(game: GameKind) -> Option<Self> {
        let refine_bu = match game {
            GameKind::Skyrim => SKYRIM_ULTRA_REFINE_BU,
            GameKind::Fallout4 => FALLOUT4_ULTRA_REFINE_BU,
            _ => return None,
        };
        Some(Self {
            refine_cells: refine_bu.iter().copied().map(cells_from_bu).collect(),
            max_cells: cells_from_bu(ULTRA_MAX_DISTANCE_BU),
        })
    }

    /// Outer extent of the whole LOD ring, in cells.
    pub(crate) fn max_cells(&self) -> i32 {
        self.max_cells
    }

    /// Coarsest level the ladder actually streams. One live band boundary
    /// means levels 4 and 8; two means 4/8/16 (Skyrim); three means the full
    /// 4/8/16/32 (FO4).
    pub(crate) fn coarsest_level(&self) -> i32 {
        LOD_LEVELS[self.refine_cells.len().min(LOD_LEVELS.len() - 1)]
    }

    /// Distance below which a level-`level` quad subdivides into four
    /// `level / 2` quads, or `None` when `level` is the finest band (4) or
    /// sits above this ladder's live boundaries.
    pub(crate) fn refine_threshold(&self, level: i32) -> Option<i32> {
        let idx = LOD_LEVELS.iter().position(|&l| l == level)?;
        // Level `LOD_LEVELS[idx]` refines at `fBlockLevel<idx-1>Distance`.
        self.refine_cells.get(idx.checked_sub(1)?).copied()
    }
}

/// Chebyshev distance in cells from `player` to the nearest cell of the
/// level-`level` quad whose SW corner is `(qx, qy)` (covering cells
/// `[qx, qx + level) × [qy, qy + level)`). `0` when the player stands
/// inside the quad.
pub(crate) fn quad_min_chebyshev(qx: i32, qy: i32, level: i32, player: (i32, i32)) -> i32 {
    let nx = player.0.clamp(qx, qx + level - 1);
    let ny = player.1.clamp(qy, qy + level - 1);
    (player.0 - nx).abs().max((player.1 - ny).abs())
}

/// Chebyshev distance from `player` to the **centre** of the level-`level`
/// quad at `(qx, qy)`, in half-cells. A quad centre falls on a half-cell
/// boundary for even `level`, so the whole computation is doubled to stay in
/// integers — callers compare against `2 * threshold`.
///
/// This is the band-selection metric; see the module docs for why it is not
/// [`quad_min_chebyshev`].
fn quad_center_chebyshev_halves(qx: i32, qy: i32, level: i32, player: (i32, i32)) -> i32 {
    let cx2 = 2 * qx + level - 1;
    let cy2 = 2 * qy + level - 1;
    (2 * player.0 - cx2).abs().max((2 * player.1 - cy2).abs())
}

/// Inputs to one [`select_lod_quads`] descent.
pub(crate) struct LodBandSelection<'a> {
    pub(crate) ladder: &'a LodBandLadder,
    pub(crate) player: (i32, i32),
    /// Worldspace-relative quad-grid origin (`lod_support::quad_origin`).
    pub(crate) grid_origin: (i32, i32),
    /// The full-detail streaming boundary (`radius_unload`). A quad is
    /// emitted only when it lies **entirely** beyond this, so baked LOD can
    /// never overlap a still-resident full-detail cell (#1866 / #1871).
    pub(crate) exclude_within: i32,
    /// Inclusive cell bounds of the worldspace, when known. Quads that miss
    /// the worldspace entirely are pruned — without this, the
    /// subdivide-on-missing-asset rule below would recurse open ocean all
    /// the way down to level 4 and burn a lookup per empty quad.
    pub(crate) world_bounds: Option<((i32, i32), (i32, i32))>,
}

/// Select the quads to stream this reconcile, as `(level, qx, qy)`.
///
/// Top-down quadtree descent from [`LodBandLadder::coarsest_level`]. Each
/// visited node either **emits itself** or **subdivides into its four
/// children** — never both — so the result is an exact partition of the
/// ring: no quad overlaps another, and the only omissions are the two
/// deliberate ones (inside `exclude_within`, or outside the ring / the
/// worldspace).
///
/// A node subdivides when either:
///   * it is nearer than its refine threshold (with hysteresis), or
///   * the game ships no baked asset for it (`available` is false), in which
///     case descending is what keeps a hole out of the horizon — the finest
///     level always emits, where the caller's own fallback (synthesized
///     terrain, or an empty sentinel for objects) takes over.
///
/// `resident` reports whether `(level, qx, qy)` was emitted at that level by
/// the previous reconcile; it supplies the hysteresis direction. A quad
/// neither resident nor previously subdivided (a first load) resolves with
/// the threshold biased one cell toward finer detail, which is stable from
/// the next reconcile onward.
pub(crate) fn select_lod_quads(
    sel: &LodBandSelection<'_>,
    resident: impl Fn(i32, i32, i32) -> bool,
    available: impl Fn(i32, i32, i32) -> bool,
) -> Vec<(i32, i32, i32)> {
    let ladder = sel.ladder;
    let coarsest = ladder.coarsest_level();
    let finest = LOD_LEVELS[0];
    let mut out = Vec::new();

    // Roots: every coarsest-level quad that can reach inside the ring.
    let (pqx, pqy) = quad_origin(sel.player.0, sel.player.1, coarsest, sel.grid_origin);
    let span = ladder.max_cells / coarsest + 1;
    let mut stack: Vec<(i32, i32, i32)> = Vec::new();
    for dj in -span..=span {
        for di in -span..=span {
            stack.push((coarsest, pqx + di * coarsest, pqy + dj * coarsest));
        }
    }

    while let Some((level, qx, qy)) = stack.pop() {
        if !quad_intersects_bounds(qx, qy, level, sel.world_bounds) {
            continue;
        }
        let d = quad_min_chebyshev(qx, qy, level, sel.player);
        if d > ladder.max_cells {
            continue; // beyond the outermost band
        }

        if level > finest {
            // Sticky in whichever direction preserves the previous decision:
            // a quad drawn at this level must get a cell closer to split, a
            // split one must retreat a cell past the threshold to merge.
            let sticky = if resident(level, qx, qy) {
                -LOD_BAND_HYSTERESIS_CELLS
            } else {
                LOD_BAND_HYSTERESIS_CELLS
            };
            // Band selection measures to the quad centre, in half-cells.
            let dc = quad_center_chebyshev_halves(qx, qy, level, sel.player);
            let too_near = ladder
                .refine_threshold(level)
                .is_some_and(|t| dc <= 2 * (t + sticky));
            if too_near || !available(level, qx, qy) {
                let half = level / 2;
                stack.push((half, qx, qy));
                stack.push((half, qx + half, qy));
                stack.push((half, qx, qy + half));
                stack.push((half, qx + half, qy + half));
                continue;
            }
        }

        // Entirely beyond the full-detail region, or it would fight the
        // still-resident near cells it overlaps.
        if d > sel.exclude_within {
            out.push((level, qx, qy));
        }
    }

    out
}

/// Whether the level-`level` quad at `(qx, qy)` touches the worldspace's
/// inclusive cell bounds. Unknown bounds accept everything.
fn quad_intersects_bounds(
    qx: i32,
    qy: i32,
    level: i32,
    bounds: Option<((i32, i32), (i32, i32))>,
) -> bool {
    let Some(((min_x, min_y), (max_x, max_y))) = bounds else {
        return true;
    };
    // The quad's inclusive max cell is `q + level - 1`; written as
    // `q + level > min` so the comparison stays clippy-clean.
    qx <= max_x && qx + level > min_x && qy <= max_y && qy + level > min_y
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn skyrim() -> LodBandLadder {
        LodBandLadder::for_game(GameKind::Skyrim).expect("Skyrim ships baked quadtree LOD")
    }

    fn fo4() -> LodBandLadder {
        LodBandLadder::for_game(GameKind::Fallout4).expect("FO4 ships baked quadtree LOD")
    }

    fn selection<'a>(ladder: &'a LodBandLadder, player: (i32, i32)) -> LodBandSelection<'a> {
        LodBandSelection {
            ladder,
            player,
            grid_origin: (0, 0),
            exclude_within: 0,
            world_bounds: None,
        }
    }

    /// Everything available, nothing resident — the plain-threshold case.
    fn plain(sel: &LodBandSelection<'_>) -> Vec<(i32, i32, i32)> {
        select_lod_quads(sel, |_, _, _| false, |_, _, _| true)
    }

    /// The thresholds are the shipped `[TerrainManager]` values converted to
    /// cells, not tuned numbers: 60000/90000/110000/250000 BU over a 4096 BU
    /// cell. A change here means a change of source data, not of taste.
    #[test]
    fn ladders_match_vanilla_terrain_manager_distances() {
        assert_eq!(cells_from_bu(60_000.0), 15);
        assert_eq!(cells_from_bu(90_000.0), 22);
        assert_eq!(cells_from_bu(110_000.0), 27);
        assert_eq!(cells_from_bu(250_000.0), 61);

        let sk = skyrim();
        assert_eq!(sk.refine_threshold(8), Some(15)); // fBlockLevel0Distance
        assert_eq!(sk.refine_threshold(16), Some(22)); // fBlockLevel1Distance
        assert_eq!(sk.refine_threshold(32), None); // Skyrim authors no Level2
        assert_eq!(sk.max_cells(), 61);

        let fo = fo4();
        assert_eq!(fo.refine_threshold(8), Some(15));
        assert_eq!(fo.refine_threshold(16), Some(22));
        assert_eq!(fo.refine_threshold(32), Some(27)); // fBlockLevel2Distance
        assert_eq!(fo.max_cells(), 61);

        // Level 4 is the finest band and never refines further.
        assert_eq!(sk.refine_threshold(4), None);
        assert_eq!(fo.refine_threshold(4), None);
    }

    /// Skyrim's missing `fBlockLevel2Distance` is what caps its runtime
    /// ladder at level 16 — matching its archives, which ship level-4/8/16
    /// `.bto` object LOD and no level-32 `.bto` at all. FO4 authors the
    /// setting and ships the level-32 tier.
    #[test]
    fn coarsest_level_follows_the_authored_band_count() {
        assert_eq!(skyrim().coarsest_level(), 16);
        assert_eq!(fo4().coarsest_level(), 32);
    }

    /// Titles with no combined `.btr` quadtree keep their existing single-band
    /// geometry schemes. FO3/FNV's older NIF/DDS tree is a separate EXAL
    /// capability (#3100), not evidence that they support this ladder.
    #[test]
    fn pre_skyrim_games_have_no_combined_btr_ladder() {
        assert!(LodBandLadder::for_game(GameKind::Oblivion).is_none());
        assert!(LodBandLadder::for_game(GameKind::Fallout3NV).is_none());
    }

    /// The core structural guarantee: the emitted quads tile the ring
    /// without a single cell being covered twice. Checked cell-by-cell over
    /// the whole ring rather than by spot-checking coordinates.
    #[test]
    fn partition_never_covers_a_cell_twice() {
        let ladder = fo4();
        let sel = selection(&ladder, (7, -3));
        let quads = plain(&sel);
        assert!(!quads.is_empty());

        let mut seen: HashSet<(i32, i32)> = HashSet::new();
        for &(level, qx, qy) in &quads {
            for dy in 0..level {
                for dx in 0..level {
                    assert!(
                        seen.insert((qx + dx, qy + dy)),
                        "cell ({}, {}) covered twice — level-{level} quad ({qx}, {qy}) \
                         overlaps another band",
                        qx + dx,
                        qy + dy,
                    );
                }
            }
        }
    }

    /// The complement of the overlap test. Past the full-detail boundary
    /// annulus, every cell in the ring is covered — the bands leave no hole
    /// in the horizon.
    ///
    /// The annulus itself is a real, pre-existing gap and this test pins its
    /// exact width rather than papering over it. A quad is emitted only when
    /// it lies *entirely* beyond `exclude_within` (#1866 / #1871), so a
    /// finest-level quad straddling that boundary is dropped whole, taking
    /// its outer cells with it — up to `LOD_LEVELS[0] - 1` cells of missing
    /// distant terrain hugging the streamed region. Closing it needs
    /// per-record VWD full-model culling so LOD and full detail can safely
    /// overlap, which is the remaining half of EX-11.
    #[test]
    fn partition_leaves_no_gap_beyond_the_full_detail_annulus() {
        let ladder = fo4();
        let mut sel = selection(&ladder, (0, 0));
        sel.exclude_within = 6; // a typical radius_unload
        let quads = plain(&sel);

        let mut covered: HashSet<(i32, i32)> = HashSet::new();
        for &(level, qx, qy) in &quads {
            for dy in 0..level {
                for dx in 0..level {
                    covered.insert((qx + dx, qy + dy));
                }
            }
        }

        let finest = LOD_LEVELS[0];
        let max = ladder.max_cells();
        for y in -max..=max {
            for x in -max..=max {
                let d = x.abs().max(y.abs());
                if d <= sel.exclude_within || d > max {
                    continue;
                }
                if covered.contains(&(x, y)) {
                    continue;
                }
                // The only permitted omission: this cell's finest-level quad
                // reaches inside the full-detail region, so the whole quad
                // was dropped.
                let (qx, qy) = quad_origin(x, y, finest, sel.grid_origin);
                assert!(
                    quad_min_chebyshev(qx, qy, finest, sel.player) <= sel.exclude_within,
                    "cell ({x}, {y}) at distance {d} is inside the ring, outside the \
                     full-detail annulus, and no band covers it"
                );
            }
        }

        // And the annulus really is bounded by one finest-level quad: past
        // it, coverage is total.
        for y in -max..=max {
            for x in -max..=max {
                let d = x.abs().max(y.abs());
                if d > sel.exclude_within + finest && d <= max {
                    assert!(
                        covered.contains(&(x, y)),
                        "cell ({x}, {y}) at distance {d} is well past the annulus but uncovered"
                    );
                }
            }
        }
    }

    /// Cells inside the full-detail streaming region are left to the near
    /// terrain — baked LOD must not be emitted over a resident cell
    /// (#1866 / #1871, now enforced for every band, not just level 4).
    #[test]
    fn quads_inside_the_full_detail_region_are_never_emitted() {
        let ladder = fo4();
        let mut sel = selection(&ladder, (0, 0));
        sel.exclude_within = 6;
        for (level, qx, qy) in plain(&sel) {
            assert!(
                quad_min_chebyshev(qx, qy, level, sel.player) > 6,
                "level-{level} quad ({qx}, {qy}) reaches inside the full-detail radius"
            );
        }
    }

    /// Detail decreases monotonically with distance: no coarse quad sits
    /// nearer than a fine one. This is what makes the horizon read as bands
    /// rather than a patchwork.
    #[test]
    fn level_increases_with_distance() {
        let ladder = fo4();
        let sel = selection(&ladder, (0, 0));
        let quads = plain(&sel);

        let mut nearest_of_level: Vec<(i32, i32)> = Vec::new();
        for &(level, qx, qy) in &quads {
            let d = quad_min_chebyshev(qx, qy, level, sel.player);
            match nearest_of_level.iter_mut().find(|(l, _)| *l == level) {
                Some((_, best)) => *best = (*best).min(d),
                None => nearest_of_level.push((level, d)),
            }
        }
        nearest_of_level.sort_unstable();
        // Level 4 starts at the player; each coarser band starts further out.
        for w in nearest_of_level.windows(2) {
            assert!(
                w[0].1 < w[1].1,
                "level {} starts at {} but coarser level {} starts at {} — bands are not ordered",
                w[0].0,
                w[0].1,
                w[1].0,
                w[1].1,
            );
        }
        assert_eq!(nearest_of_level.first().map(|&(l, _)| l), Some(4));
    }

    /// A ladder tuned so a real level-8 quad lands inside the hysteresis
    /// window. Centre distances for level-8 quads step by 8 cells (3.5,
    /// 11.5, 19.5, …), so with the shipped thresholds the 2-cell window
    /// almost never catches one — which is the point of well-separated
    /// bands, but leaves nothing for a hysteresis test to grip. Level-8
    /// refine at 12 puts the 11.5 quad squarely in `(11, 13]`.
    fn windowed_ladder() -> LodBandLadder {
        LodBandLadder {
            refine_cells: vec![12, 30, 40],
            max_cells: 61,
        }
    }

    /// Hysteresis: a quad inside the switch window keeps whatever it was
    /// doing last reconcile. Without this, a player pacing one cell back and
    /// forth across a boundary would reload a whole band every step.
    #[test]
    fn quads_inside_the_switch_window_keep_their_previous_band() {
        let ladder = windowed_ladder();
        let t = ladder.refine_threshold(8).unwrap();
        let h = LOD_BAND_HYSTERESIS_CELLS;
        let sel = selection(&ladder, (0, 0));

        let target = (8, 8, 0);
        let halves = quad_center_chebyshev_halves(target.1, target.2, 8, sel.player);
        assert_eq!(halves, 23, "centre sits 11.5 cells out");
        assert!(
            halves > 2 * (t - h) && halves <= 2 * (t + h),
            "centre distance {halves} half-cells must be in the switch window"
        );

        // Previously drawn at level 8: it stays level 8 (must come closer).
        let kept = select_lod_quads(&sel, |l, x, y| (l, x, y) == target, |_, _, _| true);
        assert!(
            kept.contains(&target),
            "a quad already drawn at level 8 must not split while inside the window"
        );

        // Previously subdivided: it stays subdivided (must retreat further).
        let split = plain(&sel);
        assert!(
            !split.contains(&target),
            "a quad already split must not merge while inside the window"
        );
    }

    /// Both hysteresis branches still converge: outside the switch window
    /// the decision is the same regardless of what the previous reconcile
    /// chose. Hysteresis must damp thrash, not pin a band forever.
    #[test]
    fn hysteresis_still_flips_outside_the_window() {
        let ladder = windowed_ladder();
        let sel = selection(&ladder, (0, 0));

        // Centre 3.5 cells, well inside the level-8 refine threshold (12):
        // splits to level 4 even though it was resident at level 8.
        let near = (8, 0, 0);
        let r = select_lod_quads(&sel, |l, x, y| (l, x, y) == near, |_, _, _| true);
        assert!(
            !r.contains(&near),
            "a quad far inside its threshold must split regardless of history"
        );

        // Centre 27.5 cells, well past it: drawn at level 8 even though it
        // was previously subdivided.
        let far = (8, 24, 0);
        assert_eq!(
            quad_center_chebyshev_halves(far.1, far.2, 8, sel.player),
            55
        );
        let r = plain(&sel);
        assert!(
            r.contains(&far),
            "a quad clearly past its threshold must merge back to level 8"
        );
    }

    /// The metric regression this module exists to avoid: every band the
    /// ladder declares must actually be reachable. Measuring band distance
    /// to a quad's nearest cell rather than its centre stranded whole bands
    /// — FO4's level-16 window is 5 cells wide while level-16 quads step 16
    /// cells at a time, so *no* player position could ever select one,
    /// orphaning the 144 level-16 `.btr` and 48 level-16 `.bto` the games
    /// ship. Centre distance makes every band reachable.
    ///
    /// Reachability is per *position*, not universal: a strict quadtree
    /// quantises a band's candidates to its own quad size, so a player
    /// standing exactly on a coarse quad corner can momentarily see a band
    /// skipped (Skyrim at the origin jumps 4 → 16). That is inherent to the
    /// partition — the shipped assets *are* a strict quadtree — and costs a
    /// slightly larger LOD pop at those positions, not a hole. What must
    /// never happen is a band that no position can reach.
    #[test]
    fn every_declared_band_is_reachable_from_some_position() {
        for (name, ladder) in [("Skyrim", skyrim()), ("FO4", fo4())] {
            let coarsest = ladder.coarsest_level();
            let mut seen: HashSet<i32> = HashSet::new();
            // Sample the player across one coarse quad — every distinct
            // phase the grid can present.
            for py in 0..coarsest {
                for px in 0..coarsest {
                    let sel = selection(&ladder, (px, py));
                    seen.extend(plain(&sel).iter().map(|&(l, _, _)| l));
                }
            }
            for &level in LOD_LEVELS.iter().filter(|&&l| l <= coarsest) {
                assert!(
                    seen.contains(&level),
                    "{name} declares a level-{level} band but no player position selects one"
                );
            }
            assert!(
                seen.iter().all(|&l| l <= coarsest),
                "{name} selected a band coarser than its ladder declares: {seen:?}"
            );
        }
    }

    /// A quad the game ships no baked mesh for subdivides instead of
    /// leaving a hole, all the way down to the finest band where the
    /// caller's own fallback takes over. Modelled on Skyrim, which ships no
    /// level-32 `.bto` and only 48 level-16 ones.
    #[test]
    fn missing_baked_asset_subdivides_instead_of_holing() {
        let ladder = fo4();
        let sel = selection(&ladder, (0, 0));

        // Nothing baked at any coarse level → everything lands at level 4.
        let quads = select_lod_quads(&sel, |_, _, _| false, |level, _, _| level == 4);
        assert!(!quads.is_empty());
        assert!(
            quads.iter().all(|&(level, _, _)| level == 4),
            "with no coarse assets every quad must descend to the finest band"
        );

        // Only level 16 and finer baked → nothing coarser than 16 survives.
        let quads = select_lod_quads(&sel, |_, _, _| false, |level, _, _| level <= 16);
        assert!(quads.iter().all(|&(level, _, _)| level <= 16));
        assert!(quads.iter().any(|&(level, _, _)| level == 16));
    }

    /// Availability-driven subdivision must not run away over open ocean:
    /// quads that miss the worldspace entirely are pruned before they cost
    /// a lookup, so a small worldspace does not spawn a level-4 quad for
    /// every cell out to the 61-cell ring.
    #[test]
    fn quads_outside_the_worldspace_are_pruned() {
        let ladder = fo4();
        let bounds = ((-8, -8), (7, 7));
        let sel = LodBandSelection {
            ladder: &ladder,
            player: (0, 0),
            grid_origin: (0, 0),
            exclude_within: 0,
            world_bounds: Some(bounds),
        };
        let quads = select_lod_quads(&sel, |_, _, _| false, |level, _, _| level == 4);
        assert!(!quads.is_empty());
        for (level, qx, qy) in quads {
            assert!(
                quad_intersects_bounds(qx, qy, level, Some(bounds)),
                "level-{level} quad ({qx}, {qy}) lies wholly outside the worldspace"
            );
        }
    }

    /// Quads stay aligned to the worldspace's own grid origin, not global
    /// zero — the #2586 invariant, now across every band.
    #[test]
    fn every_band_keeps_the_worldspace_grid_phase() {
        let ladder = fo4();
        let origin = (-50, -50);
        let sel = LodBandSelection {
            ladder: &ladder,
            player: (-49, -49),
            grid_origin: origin,
            exclude_within: 0,
            world_bounds: None,
        };
        for (level, qx, qy) in plain(&sel) {
            assert_eq!((qx - origin.0).rem_euclid(level), 0);
            assert_eq!((qy - origin.1).rem_euclid(level), 0);
        }
    }

    /// Selection is deterministic — the same inputs must not reorder or
    /// change size between runs (the streaming reconcile diffs against it).
    #[test]
    fn selection_is_deterministic() {
        let ladder = fo4();
        let sel = selection(&ladder, (3, 11));
        let a = plain(&sel);
        let b = plain(&sel);
        assert_eq!(a, b);
    }

    #[test]
    fn quad_min_chebyshev_is_zero_inside_the_quad() {
        assert_eq!(quad_min_chebyshev(0, 0, 4, (2, 2)), 0);
        assert_eq!(quad_min_chebyshev(0, 0, 4, (3, 3)), 0);
        // One cell east of the quad's east edge.
        assert_eq!(quad_min_chebyshev(0, 0, 4, (4, 0)), 1);
        assert_eq!(quad_min_chebyshev(0, 0, 4, (-1, 0)), 1);
    }
}
