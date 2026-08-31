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

// Fallout 3's shipped `VeryHigh.ini` uses the legacy terrain-manager names:
// `fBlockLoadDistanceLow=50000`, `fSplitDistanceMult=1.5`, and
// `fBlockLoadDistance=125000`. The split multiplier generates the successive
// quadtree boundaries. Its archives author level-4/8/16/32 quads through
// +/-64 cells, which is the availability-clamped outer extent. FNV shares the
// same format and `GameKind`; missing quads subdivide through the common
// availability predicate — except on the object ring, where a quad whose
// whole subtree is unbaked coarsens instead (#3502). FNV bakes every
// worldspace's `blocks\` quads at level 4 and never needs that; FO3 bakes 93
// of 422 at level 8 only.
const FALLOUT_LEGACY_REFINE_BU: &[f32] = &[50_000.0, 75_000.0, 112_500.0];
const FALLOUT_LEGACY_MAX_CELLS: i32 = 64;

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
    /// The **Creation-format** (`.bto` / `.btr`) ladder for `game`, or
    /// `None` for titles that ship none (Oblivion / FO3 / FNV). Prefer
    /// [`Self::for_terrain_game`] / [`Self::for_object_game`], which fall
    /// back to the FO3/FNV `landscape\lod` quadtree; this raw form is the
    /// Creation-only half they dispatch to.
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

    /// Terrain ladder, including the older FO3/FNV NIF/DDS quadtree.
    pub(crate) fn for_terrain_game(game: GameKind) -> Option<Self> {
        if matches!(game, GameKind::Fallout3NV) {
            return Some(Self::fallout_legacy());
        }
        Self::for_game(game)
    }

    /// Object-LOD ladder. Skyrim/FO4 use the Creation-format `.bto` bands
    /// from [`Self::for_game`]; FO3/FNV use the same legacy quadtree their
    /// terrain LOD does, because their object LOD is a *sibling directory*
    /// of it (`meshes\landscape\lod\<world>\blocks\`) sharing the
    /// `level<L>.x<qx>.y<qy>` naming and quad grid — see
    /// [`super::object_lod::ObjectLodScheme`].
    ///
    /// This deliberately reverses the note that used to sit on
    /// [`Self::for_terrain_game`] ("object LOD continues to use `for_game`,
    /// because Fallout's legacy terrain bands are not Creation-format
    /// `.bto` bands"). That was written under the #2086 premise that FO3/FNV
    /// ship no object LOD at all, so the only question was which ladder an
    /// unused code path should take. The archive falsifies the premise
    /// (#3321), and the legacy bands are exactly the right ones for a
    /// quadtree that shares the terrain quads' grid.
    pub(crate) fn for_object_game(game: GameKind) -> Option<Self> {
        if matches!(game, GameKind::Fallout3NV) {
            return Some(Self::fallout_legacy());
        }
        Self::for_game(game)
    }

    /// The FO3/FNV `landscape\lod` quadtree ladder, shared by that family's
    /// terrain and object LOD.
    fn fallout_legacy() -> Self {
        Self {
            refine_cells: FALLOUT_LEGACY_REFINE_BU
                .iter()
                .copied()
                .map(cells_from_bu)
                .collect(),
            max_cells: FALLOUT_LEGACY_MAX_CELLS,
        }
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
    /// Whether a quad may **coarsen** rather than subdivide when no finer
    /// asset exists anywhere beneath it (#3502).
    ///
    /// The two callers differ in what happens at the bottom of the descent,
    /// and that difference is the whole reason this flag exists:
    ///
    /// * **Terrain** (`false`): its availability predicate reports the
    ///   finest level available unconditionally, because heightmap synthesis
    ///   can cover any footprint. The descent therefore always terminates on
    ///   a quad that can draw, and subdividing into a missing asset is the
    ///   *right* answer — it buys a synthesized quad at higher detail.
    /// * **Objects** (`true`): the fallback for a missing quad is
    ///   `ObjectLodBlock::empty()`, a "nothing here" sentinel. Subdividing
    ///   into an absent asset draws nothing at all, so a quad whose subtree
    ///   is entirely unbaked must emit *itself* while it still can.
    ///
    /// FNV never exposed the difference — every FNV worldspace bakes its
    /// `blocks\` quads at level 4, the finest, which always emits. FO3 bakes
    /// 93 of its 422 object quads at level 8 with no level-4 sibling
    /// (`WashMonTop`'s 65, `ParadiseFalls`, five `DCworld*`), and those
    /// worldspaces lost every distant building in the ~5..15-cell band
    /// whenever the streaming radius put `exclude_within` below
    /// `cells_from_bu(FALLOUT_LEGACY_REFINE_BU[0])` = 12.
    pub(crate) coarsen_to_available: bool,
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
/// …with one exception, [`LodBandSelection::coarsen_to_available`] (#3502):
/// a node that *can* draw and whose entire subtree cannot must emit itself
/// instead, because for the object ring "descend" and "draw nothing" are the
/// same instruction.
///
/// `resident` reports whether `(level, qx, qy)` was emitted at that level by
/// the previous reconcile; it supplies the hysteresis direction. A quad
/// neither resident nor previously subdivided (a first load) resolves with
/// the threshold biased one cell toward finer detail, which is stable from
/// the next reconcile onward.
pub(crate) fn select_lod_quads(
    sel: &LodBandSelection<'_>,
    resident: impl Fn(i32, i32, i32) -> bool,
    // `FnMut` so callers can memoise the probe behind it (#3385): archive
    // presence is a pure function of (worldspace, level, qx, qy) and the
    // opened archive set, none of which change for the life of a
    // `WorldStreamingState`, yet the descent re-ran it every reconcile frame.
    mut available: impl FnMut(i32, i32, i32) -> bool,
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
                // #3502 — the coarsen escape. Only reachable when this quad
                // is itself available (an unavailable parent has nothing to
                // fall back *to*, so it descends as before and lets the
                // caller's bottom-of-descent fallback answer), and only
                // after proving no finer asset exists anywhere beneath it.
                // The subtree probe rides the same memo as the descent's own
                // `available` calls, so a quad costs its probes once for the
                // life of the streaming state, not once per reconcile.
                let coarsen = sel.coarsen_to_available
                    && available(level, qx, qy)
                    && !any_available_below(
                        level,
                        qx,
                        qy,
                        finest,
                        sel.world_bounds,
                        &mut available,
                    );
                if !coarsen {
                    let half = level / 2;
                    stack.push((half, qx, qy));
                    stack.push((half, qx + half, qy));
                    stack.push((half, qx, qy + half));
                    stack.push((half, qx + half, qy + half));
                    continue;
                }
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

/// Whether any quad **strictly finer** than `level` inside the level-`level`
/// quad at `(qx, qy)` has a baked asset (#3502).
///
/// Answers the only question the coarsen escape needs: is there anything to
/// be gained by descending? A single-level child probe would be cheaper but
/// wrong for a ladder that skips a level — a worldspace baking 16 and 4 but
/// not 8 must still descend past the empty 8 band, which is exactly the case
/// a "no available children ⇒ coarsen" rule would break.
///
/// Off-worldspace children are skipped rather than probed, mirroring the
/// descent's own pruning, so a coastal quad does not pay for open ocean.
fn any_available_below(
    level: i32,
    qx: i32,
    qy: i32,
    finest: i32,
    bounds: Option<((i32, i32), (i32, i32))>,
    available: &mut impl FnMut(i32, i32, i32) -> bool,
) -> bool {
    let mut stack = vec![(level, qx, qy)];
    while let Some((l, x, y)) = stack.pop() {
        if l <= finest {
            continue;
        }
        let half = l / 2;
        for (cx, cy) in [(x, y), (x + half, y), (x, y + half), (x + half, y + half)] {
            if !quad_intersects_bounds(cx, cy, half, bounds) {
                continue;
            }
            if available(half, cx, cy) {
                return true;
            }
            stack.push((half, cx, cy));
        }
    }
    false
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
            coarsen_to_available: false,
        }
    }

    /// Everything available, nothing resident — the plain-threshold case.
    fn plain(sel: &LodBandSelection<'_>) -> Vec<(i32, i32, i32)> {
        select_lod_quads(sel, |_, _, _| false, |_, _, _| true)
    }

    /// #3385 — the availability probe is a pure function of its key, so
    /// memoising it must not change the descent's answer, and the descent
    /// must not need the underlying probe more than once per distinct quad.
    ///
    /// This pins the contract the caller-side memo relies on. The memo
    /// itself lives on `WorldStreamingState` and its end-to-end behaviour
    /// needs a live provider, so what is checkable without a device is
    /// exactly this: same result, no repeat probes.
    #[test]
    fn memoising_the_availability_probe_preserves_the_selection() {
        use std::collections::HashMap;

        let ladder = skyrim();
        let sel = selection(&ladder, (12, -7));

        // A deterministic, non-trivial availability pattern — some quads
        // baked, some not, so the descent actually subdivides.
        let probe = |level: i32, qx: i32, qy: i32| (level + qx.abs() + qy.abs()) % 3 != 0;

        let direct = select_lod_quads(&sel, |_, _, _| false, |l, x, y| probe(l, x, y));

        let mut cache: HashMap<(i32, i32, i32), bool> = HashMap::new();
        let mut calls: HashMap<(i32, i32, i32), usize> = HashMap::new();
        let memoised = select_lod_quads(
            &sel,
            |_, _, _| false,
            |l, x, y| {
                *cache.entry((l, x, y)).or_insert_with(|| {
                    *calls.entry((l, x, y)).or_insert(0) += 1;
                    probe(l, x, y)
                })
            },
        );

        assert_eq!(
            direct, memoised,
            "memoising a pure predicate changed the selected quad set"
        );
        assert!(
            !calls.is_empty(),
            "the pattern must actually exercise the probe, or this is vacuous"
        );
        assert!(
            calls.values().all(|&n| n == 1),
            "a memoised probe was evaluated more than once for some quad: {:?}",
            calls.iter().filter(|(_, &n)| n > 1).collect::<Vec<_>>()
        );
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

    #[test]
    fn fallout_legacy_terrain_uses_all_four_authored_bands() {
        let fo3 = LodBandLadder::for_terrain_game(GameKind::Fallout3NV).unwrap();
        assert_eq!(fo3.refine_threshold(8), Some(12));
        assert_eq!(fo3.refine_threshold(16), Some(18));
        assert_eq!(fo3.refine_threshold(32), Some(27));
        assert_eq!(fo3.coarsest_level(), 32);
        assert_eq!(fo3.max_cells(), 64);
    }

    /// Titles with no combined `.btr` quadtree stay out of the object ladder.
    /// FO3/FNV's older NIF/DDS tree is exposed only by `for_terrain_game`.
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
            coarsen_to_available: false,
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
            coarsen_to_available: false,
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

    // ── #3502 (FO3-2026-08-27-D4-01) — the object ring's coarsen escape.

    fn fallout_legacy_ladder() -> LodBandLadder {
        LodBandLadder::for_object_game(GameKind::Fallout3NV)
            .expect("FO3/FNV ship the landscape\\lod quadtree")
    }

    /// A worldspace shaped like `WashMonTop`: object quads baked at level 8
    /// only, no level-4 sibling anywhere.
    fn level_8_only(sel: &LodBandSelection<'_>) -> Vec<(i32, i32, i32)> {
        select_lod_quads(sel, |_, _, _| false, |level, _, _| level == 8)
    }

    /// The bug: with objects' "missing quad ⇒ empty sentinel" fallback, the
    /// descent asked for level-4 quads that cannot exist on 7 of FO3's 15
    /// worldspaces, and the band they were meant to fill drew nothing.
    ///
    /// `exclude_within = 4` is `--radius 3`, the radius the exterior smoke
    /// and bench recipes use. The pre-fix selection here was 55 level-4
    /// requests spanning cells 5..15, every one of them absent.
    #[test]
    fn object_ring_coarsens_instead_of_asking_for_unbaked_finer_quads() {
        let ladder = fallout_legacy_ladder();
        let sel = LodBandSelection {
            ladder: &ladder,
            player: (0, 0),
            grid_origin: (0, 0),
            exclude_within: 4,
            world_bounds: None,
            coarsen_to_available: true,
        };
        let quads = level_8_only(&sel);

        assert!(
            quads.iter().all(|&(level, _, _)| level != 4),
            "a level-8-only worldspace must never be asked for a level-4 quad: {:?}",
            quads
                .iter()
                .filter(|&&(l, _, _)| l == 4)
                .collect::<Vec<_>>()
        );
        // …and the band it used to hollow out now draws. Pre-fix the
        // nearest quad that could actually emit geometry was the level-8
        // one at 16 cells: every level-8 quad closer than
        // `refine_threshold(8)` = 12 subdivided into absent level-4
        // children. Post-fix those quads emit themselves, so coverage
        // starts at the first level-8 quad that clears `exclude_within`.
        //
        // 8, not 5: the quad the player stands in spans cells 0..7 and is
        // dropped whole by the #1866 / #1871 containment rule, which is
        // cross-game and deliberate — this fix closes the 8..15 hole, not
        // that one.
        let nearest = quads
            .iter()
            .map(|&(level, qx, qy)| quad_min_chebyshev(qx, qy, level, sel.player))
            .min()
            .expect("the ring must not be empty");
        assert_eq!(
            nearest, 8,
            "the 8..15-cell band is still hollow — nearest drawable quad is {nearest} cells out"
        );
        assert!(
            quads
                .iter()
                .any(|&(level, qx, qy)| level == 8
                    && quad_min_chebyshev(qx, qy, level, sel.player) < 16),
            "no level-8 quad inside the old 16-cell floor"
        );
    }

    /// The terrain arm must keep subdividing: its availability predicate
    /// reports the finest level available unconditionally, so descending is
    /// how it buys a synthesized quad at higher detail. Same inputs, other
    /// policy, opposite answer — this is the difference the flag encodes,
    /// and it is what makes the fix object-only rather than a global change
    /// to the descent.
    #[test]
    fn the_terrain_policy_still_descends_into_unbaked_levels() {
        let ladder = fallout_legacy_ladder();
        let sel = LodBandSelection {
            ladder: &ladder,
            player: (0, 0),
            grid_origin: (0, 0),
            exclude_within: 4,
            world_bounds: None,
            coarsen_to_available: false,
        };
        assert!(
            level_8_only(&sel).iter().any(|&(level, _, _)| level == 4),
            "the terrain policy must still reach level 4 (heightmap synthesis covers it)"
        );
    }

    /// The coarsen escape is a fallback, not a preference: where a finer
    /// asset exists, the descent must still reach it. Pinned because the
    /// cheap version of this fix ("no available children ⇒ coarsen") gets
    /// this case wrong for a ladder that skips a level.
    #[test]
    fn coarsening_never_shadows_a_finer_asset_that_exists() {
        let ladder = fallout_legacy_ladder();
        let sel = LodBandSelection {
            ladder: &ladder,
            player: (0, 0),
            grid_origin: (0, 0),
            exclude_within: 4,
            world_bounds: None,
            coarsen_to_available: true,
        };
        // Everything baked: identical to the pre-#3502 selection.
        assert_eq!(
            plain(&sel),
            select_lod_quads(&sel, |_, _, _| false, |_, _, _| true)
        );
        assert!(plain(&sel).iter().any(|&(level, _, _)| level == 4));

        // A worldspace that bakes 16 and 4 but skips 8 — the level-16 quad
        // has no available child, yet a level-4 grandchild exists, so it
        // must descend past the empty band rather than coarsen onto it.
        let split = select_lod_quads(&sel, |_, _, _| false, |level, _, _| level != 8);
        assert!(
            split.iter().any(|&(level, _, _)| level == 4),
            "a skipped intermediate band must not stop the descent"
        );
    }

    /// The escape must not smuggle a quad past `exclude_within`: a coarse
    /// quad that reaches inside the full-detail ring is still dropped
    /// whole, per #1866 / #1871.
    #[test]
    fn coarsened_quads_still_respect_the_full_detail_boundary() {
        let ladder = fallout_legacy_ladder();
        for exclude_within in [0, 4, 6, 13] {
            let sel = LodBandSelection {
                ladder: &ladder,
                player: (0, 0),
                grid_origin: (0, 0),
                exclude_within,
                world_bounds: None,
                coarsen_to_available: true,
            };
            for (level, qx, qy) in level_8_only(&sel) {
                assert!(
                    quad_min_chebyshev(qx, qy, level, sel.player) > exclude_within,
                    "level-{level} quad ({qx}, {qy}) reaches inside exclude_within \
                     {exclude_within}"
                );
            }
        }
    }

    /// The escape stays inside the partition contract: emitting the parent
    /// covers exactly the footprint its four children would have, so no two
    /// emitted quads overlap.
    #[test]
    fn coarsened_selection_is_still_a_partition() {
        let ladder = fallout_legacy_ladder();
        let sel = LodBandSelection {
            ladder: &ladder,
            player: (0, 0),
            grid_origin: (0, 0),
            exclude_within: 4,
            world_bounds: None,
            coarsen_to_available: true,
        };
        let mut covered: HashSet<(i32, i32)> = HashSet::new();
        for (level, qx, qy) in level_8_only(&sel) {
            for y in qy..qy + level {
                for x in qx..qx + level {
                    assert!(
                        covered.insert((x, y)),
                        "cell ({x}, {y}) is covered twice — level-{level} quad \
                         ({qx}, {qy}) overlaps an earlier one"
                    );
                }
            }
        }
    }
}
