//! Live-residency coverage auditing for distant LOD (EX-10/11 / #2371).
//!
//! `lod_bands::select_lod_quads` and its `partition_*` tests already prove
//! the *desired* quad set for one band ladder has no gaps or overlaps by
//! construction. What they cannot prove is that the *live* `HashMap`s a real
//! run actually holds — after every load / unload / reconcile / boundary
//! crossing across a real traversal — still matches that guarantee: a
//! missed unload, a stale key surviving a reconcile, or a boundary
//! miscalculation could leave two resident quads (or a resident quad and a
//! still-resident full-detail cell) claiming the same ground without the
//! construction proof ever running against that state. This module checks
//! live residency instead of re-deriving the desired set, so it protects
//! against exactly the class of bug the construction proof cannot see:
//! state drifting away from the invariant, not the invariant itself being
//! wrong.
//!
//! Two checks, both pure functions/state over plain key sets so they're
//! testable without a `World` / `VulkanContext`:
//!   - **Overlap** ([`find_overlaps`], [`find_full_detail_overlaps`]): any
//!     two resident quad footprints (same scheme, or a resident quad against
//!     a resident full-detail cell) that share a cell.
//!   - **Churn** ([`ChurnTracker`]): a quad key that left residency and came
//!     back — real thrash, distinct from `streaming::StreamingTelemetry`'s
//!     `superseded_*` counters, which only catch one in-flight load being
//!     cancelled by the *next* boundary crossing, not a settled key flapping
//!     across several boundaries.
//!
//! "Holes" (a desired coordinate neither resident nor a confirmed miss) are
//! deliberately NOT re-derived here — `LodReconcileProgress::complete`
//! (`streaming_helpers.rs`) already answers that question from inside the
//! one place that computes the desired set, and re-deriving it a second time
//! here would risk the two diverging. The reconcile call site folds that
//! signal into `LodCoverageStats::settled` instead of this module computing
//! its own copy.

use std::collections::{HashMap, HashSet};

/// Inclusive cell-space rectangle `(min_x, min_y, max_x, max_y)` a quad (or
/// a single full-detail cell, `level == 1`) occupies. Matches the `(qx, qy)`
/// = southwest-corner, `level` = side-length-in-cells convention
/// `lod_bands::quad_min_chebyshev` already uses.
fn quad_rect(qx: i32, qy: i32, level: i32) -> (i32, i32, i32, i32) {
    (qx, qy, qx + level - 1, qy + level - 1)
}

fn rects_overlap(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    a.0 <= b.2 && b.0 <= a.2 && a.1 <= b.3 && b.1 <= a.3
}

/// Count resident quad pairs (same scheme) whose footprints intersect.
/// `O(n^2)` over the resident set, which real runs keep in the tens — the
/// ring radius bounds it structurally (`terrain_lod::MAX_LOD_RING_REACH_CELLS`).
pub(crate) fn find_overlaps(keys: &[(i32, i32, i32)]) -> u32 {
    let mut overlaps = 0u32;
    for (i, &(level_a, xa, ya)) in keys.iter().enumerate() {
        let rect_a = quad_rect(xa, ya, level_a);
        for &(level_b, xb, yb) in &keys[i + 1..] {
            if rects_overlap(rect_a, quad_rect(xb, yb, level_b)) {
                overlaps += 1;
            }
        }
    }
    overlaps
}

/// Count LOD quads whose footprint intersects a still-resident full-detail
/// cell — the cross-scheme boundary `object_lod`/`terrain_lod` both keep
/// conservative (gated on `radius_unload`, not `radius_load`; #1866/#1871)
/// specifically to avoid this ever happening.
#[cfg_attr(not(test), allow(dead_code))] // production counts hole-aware via `find_terrain_full_detail_overlaps`
pub(crate) fn find_full_detail_overlaps(
    lod_keys: &[(i32, i32, i32)],
    full_cells: &[(i32, i32)],
) -> u32 {
    let mut overlaps = 0u32;
    for &(level, qx, qy) in lod_keys {
        let quad = quad_rect(qx, qy, level);
        for &(cx, cy) in full_cells {
            if rects_overlap(quad, quad_rect(cx, cy, 1)) {
                overlaps += 1;
            }
        }
    }
    overlaps
}

/// Count terrain LOD/full-detail intersections that remain drawable after the
/// finest-band block's per-cell hole mask is applied. Finest terrain blocks
/// intentionally span the full-detail ring; the uploaded mesh cuts those
/// cells out, so their raw rectangle is not itself a rendered overlap.
pub(crate) fn find_terrain_full_detail_overlaps(
    lod_keys: &[((i32, i32, i32), u16)],
    full_cells: &[(i32, i32)],
) -> u32 {
    let mut overlaps = 0u32;
    for &((level, qx, qy), hole_mask) in lod_keys {
        let quad = quad_rect(qx, qy, level);
        for &(cx, cy) in full_cells {
            if !rects_overlap(quad, quad_rect(cx, cy, 1)) {
                continue;
            }
            let holed = level == 4
                && (0..4).contains(&(cx - qx))
                && (0..4).contains(&(cy - qy))
                && (hole_mask & (1u16 << ((cy - qy) * 4 + (cx - qx)))) != 0;
            if !holed {
                overlaps += 1;
            }
        }
    }
    overlaps
}

/// Mask of a finest-level (4x4-cell) quad's cells that are resident at full detail: bit
/// `(cy - qy) * 4 + (cx - qx)` is set for every resident cell `(cx, cy)` inside the quad whose south-west cell
/// is `(qx, qy)`. `0xFFFF` means the quad is entirely covered by full detail. DLOD-OBJ-002 / DLOD-TREE-008:
/// a quad that straddles the resident boundary is drawn minus these cells instead of not at all.
pub(crate) fn quad_resident_mask(qx: i32, qy: i32, resident: &HashSet<(i32, i32)>) -> u16 {
    let mut mask = 0u16;
    for dy in 0..4 {
        for dx in 0..4 {
            if resident.contains(&(qx + dx, qy + dy)) {
                mask |= 1u16 << (dy * 4 + dx);
            }
        }
    }
    mask
}

/// Which cell of the finest-level quad at `(qx, qy)` the point `(wx, wy)` (world Z-up x and y, Bethesda units)
/// falls in, as its bit in a [`quad_resident_mask`] mask. `None` outside the quad.
pub(crate) fn quad_cell_bit(qx: i32, qy: i32, wx: f32, wy: f32) -> Option<u16> {
    const CELL: f32 = 4096.0;
    let (cx, cy) = ((wx / CELL).floor() as i32, (wy / CELL).floor() as i32);
    let (dx, dy) = (cx - qx, cy - qy);
    ((0..4).contains(&dx) && (0..4).contains(&dy)).then(|| 1u16 << (dy * 4 + dx))
}

/// DLOD-GATE-005 — count a family's moat cells.
///
/// A moat cell (PRD §2) is a cell that is not a resident full-detail cell, lies within `edge` cells
/// (Chebyshev) of one — `edge` is the family's finest quad edge — inside the family's ring (`ring_cells` from
/// `player`), and is covered by no resident quad of the family. It is the gap whole-quad exclusion leaves at
/// the resident boundary, which the overlap counters cannot see.
///
/// `unauthored(x, y)` marks cells the family has no content for (a confirmed missing quad): those are not a
/// defect of the join, so they are not counted. `covered` holds the family's resident quads as
/// `(level, x, y)` south-west-corner rects.
pub(crate) fn count_moat_cells(
    resident: &HashSet<(i32, i32)>,
    covered: &[(i32, i32, i32)],
    player: (i32, i32),
    ring_cells: i32,
    edge: i32,
    unauthored: impl Fn(i32, i32) -> bool,
) -> u32 {
    let mut candidates: HashSet<(i32, i32)> = HashSet::new();
    for &(rx, ry) in resident {
        for dx in -edge..=edge {
            for dy in -edge..=edge {
                let cell = (rx + dx, ry + dy);
                if !resident.contains(&cell) {
                    candidates.insert(cell);
                }
            }
        }
    }
    let rects: Vec<(i32, i32, i32, i32)> = covered
        .iter()
        .map(|&(level, qx, qy)| quad_rect(qx, qy, level))
        .collect();
    candidates
        .into_iter()
        .filter(|&(cx, cy)| {
            (cx - player.0).abs().max((cy - player.1).abs()) <= ring_cells
                && !rects.iter().any(|&r| rects_overlap(r, quad_rect(cx, cy, 1)))
                && !unauthored(cx, cy)
        })
        .count() as u32
}

/// Tracks quad keys that left residency and later came back — real thrash
/// across a traversal, as opposed to `streaming::StreamingTelemetry`'s
/// `superseded_*` counters, which only catch one in-flight load being
/// cancelled by the next boundary crossing. A key that unloads and reloads
/// three boundaries later never supersedes anything, but it is exactly the
/// symptom EX-11's band hysteresis (one cell of margin, `d96110eb`) exists
/// to prevent — this is the runtime check that the margin is actually
/// holding on a real traversal.
///
/// One instance per LOD scheme (`WorldStreamingState` holds one for terrain,
/// one for objects) — a key space shared across schemes would conflate two
/// unrelated churn sources.
#[derive(Debug, Default, Clone)]
pub(crate) struct ChurnTracker {
    last_resident: HashSet<(i32, i32, i32)>,
    ever_evicted: HashSet<(i32, i32, i32)>,
    churned: u32,
}

impl ChurnTracker {
    /// Diff the current resident key set against the previous call's,
    /// updating the evicted/churned bookkeeping. Call once per reconcile
    /// with the scheme's live `HashMap`; `V` is unconstrained so this works
    /// for `LodBlock` and `ObjectLodBlock` alike without a shared trait.
    pub(crate) fn observe<V>(&mut self, resident: &HashMap<(i32, i32, i32), V>) {
        self.observe_keys(resident.keys().copied().collect());
    }

    /// [`Self::observe`] over an already-built key set, for a ring whose
    /// residency is not a `(level, x, y)` map (the tree ring, keyed by SW cell).
    pub(crate) fn observe_keys(&mut self, current: HashSet<(i32, i32, i32)>) {
        for key in self.last_resident.difference(&current) {
            self.ever_evicted.insert(*key);
        }
        for key in current.difference(&self.last_resident) {
            if self.ever_evicted.remove(key) {
                self.churned = self.churned.saturating_add(1);
            }
        }
        self.last_resident = current;
    }

    pub(crate) fn churned(&self) -> u32 {
        self.churned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_same_level_quads_do_not_overlap() {
        // Two level-4 quads sharing an edge, not a cell: [0,3] and [4,7].
        assert_eq!(find_overlaps(&[(4, 0, 0), (4, 4, 0)]), 0);
    }

    #[test]
    fn coarser_quad_containing_a_finer_one_is_an_overlap() {
        // Real quadtree descent never produces this (a node either emits
        // itself or recurses, never both) — this is exactly the synthetic
        // "bad" state the checker exists to catch if that invariant ever
        // drifts between construction and live residency.
        assert_eq!(find_overlaps(&[(4, 0, 0), (8, 0, 0)]), 1);
    }

    #[test]
    fn duplicate_key_pair_still_reports_an_overlap() {
        // Duplicate keys can't reach this from a real HashMap, but the pure
        // geometry function must not special-case index adjacency away.
        assert_eq!(find_overlaps(&[(4, 0, 0), (4, 0, 0)]), 1);
    }

    #[test]
    fn three_mutually_disjoint_quads_report_zero() {
        assert_eq!(
            find_overlaps(&[(4, 0, 0), (4, 4, 0), (4, 0, 4), (4, 4, 4)]),
            0
        );
    }

    #[test]
    fn overlap_count_is_the_pair_count_not_the_offending_quad_count() {
        // (4,0,0) covers [0,3]x[0,3]; (4,4,0) covers [4,7]x[0,3] — disjoint
        // from each other. (8,0,0) covers [0,7]x[0,7], which contains both
        // — 2 overlapping pairs, even though only 3 quads are involved.
        assert_eq!(find_overlaps(&[(4, 0, 0), (4, 4, 0), (8, 0, 0)]), 2);
    }

    #[test]
    fn full_detail_cell_inside_a_resident_lod_quad_is_flagged() {
        assert_eq!(find_full_detail_overlaps(&[(4, 0, 0)], &[(2, 2)]), 1);
    }

    #[test]
    fn full_detail_cell_outside_every_lod_quad_is_clean() {
        assert_eq!(find_full_detail_overlaps(&[(4, 0, 0)], &[(10, 10)]), 0);
    }

    #[test]
    fn full_detail_cell_on_a_quad_edge_counts_as_overlap() {
        // Inclusive rectangles: (3,3) is the quad's own max corner.
        assert_eq!(find_full_detail_overlaps(&[(4, 0, 0)], &[(3, 3)]), 1);
        assert_eq!(find_full_detail_overlaps(&[(4, 0, 0)], &[(4, 4)]), 0);
    }

    #[test]
    fn terrain_hole_mask_suppresses_expected_full_detail_intersection() {
        let mask = 1u16 << (2 * 4 + 2);
        assert_eq!(
            find_terrain_full_detail_overlaps(&[((4, 0, 0), mask)], &[(2, 2)]),
            0
        );
        assert_eq!(
            find_terrain_full_detail_overlaps(&[((4, 0, 0), mask)], &[(1, 1)]),
            1
        );
    }

    #[test]
    fn churn_tracker_is_silent_on_first_load() {
        let mut tracker = ChurnTracker::default();
        let mut resident = HashMap::new();
        resident.insert((4, 0, 0), ());
        tracker.observe(&resident);
        assert_eq!(tracker.churned(), 0);
    }

    #[test]
    fn churn_tracker_flags_a_key_that_leaves_and_returns() {
        let mut tracker = ChurnTracker::default();
        let mut resident = HashMap::new();
        resident.insert((4, 0, 0), ());
        tracker.observe(&resident);

        resident.clear();
        tracker.observe(&resident); // evicted

        resident.insert((4, 0, 0), ());
        tracker.observe(&resident); // back — thrash
        assert_eq!(tracker.churned(), 1);
    }

    #[test]
    fn churn_tracker_does_not_flag_a_key_that_stays_resident() {
        let mut tracker = ChurnTracker::default();
        let mut resident = HashMap::new();
        resident.insert((4, 0, 0), ());
        tracker.observe(&resident);
        tracker.observe(&resident);
        tracker.observe(&resident);
        assert_eq!(tracker.churned(), 0);
    }

    #[test]
    fn churn_tracker_does_not_flag_a_key_that_leaves_and_stays_gone() {
        let mut tracker = ChurnTracker::default();
        let mut resident = HashMap::new();
        resident.insert((4, 0, 0), ());
        tracker.observe(&resident);
        resident.clear();
        tracker.observe(&resident);
        tracker.observe(&resident);
        assert_eq!(tracker.churned(), 0);
    }

    #[test]
    fn churn_tracker_counts_repeated_flapping_of_the_same_key() {
        let mut tracker = ChurnTracker::default();
        let mut resident = HashMap::new();
        for _ in 0..3 {
            resident.insert((4, 0, 0), ());
            tracker.observe(&resident); // load — churn only if previously evicted
            resident.clear();
            tracker.observe(&resident); // evict
        }
        resident.insert((4, 0, 0), ());
        tracker.observe(&resident); // one more load after the loop's last evict
                                    // 4 loads total; the very first is a fresh load (nothing evicted
                                    // yet), so it doesn't count. Each of the other 3 follows an evict —
                                    // 3 flap-backs.
        assert_eq!(tracker.churned(), 3);
    }

    #[test]
    fn churn_tracker_schemes_are_independent() {
        let mut terrain = ChurnTracker::default();
        let object = ChurnTracker::default();
        let mut resident = HashMap::new();
        resident.insert((4, 0, 0), ());
        terrain.observe(&resident);
        terrain.observe(&HashMap::<(i32, i32, i32), ()>::new());
        terrain.observe(&resident); // terrain churns

        assert_eq!(terrain.churned(), 1);
        assert_eq!(object.churned(), 0);
    }

    #[test]
    fn moat_cells_are_the_uncovered_ring_cells_beside_the_resident_set() {
        // One resident cell at the origin, a level-4 quad edge of 4: its 9x9 neighbourhood minus itself.
        let resident: HashSet<(i32, i32)> = [(0, 0)].into_iter().collect();
        let all = count_moat_cells(&resident, &[], (0, 0), 100, 4, |_, _| false);
        assert_eq!(all, 9 * 9 - 1);
        // A quad covering x 0..=3, y 0..=3 removes the cells it holds, apart from the resident one.
        let covered = count_moat_cells(&resident, &[(4, 0, 0)], (0, 0), 100, 4, |_, _| false);
        assert_eq!(covered, 9 * 9 - 1 - 15);
        // A ring that ends at 2 cells keeps only the 5x5 neighbourhood.
        let ringed = count_moat_cells(&resident, &[], (0, 0), 2, 4, |_, _| false);
        assert_eq!(ringed, 5 * 5 - 1);
        // Cells the family has no content for are not part of the join defect.
        let unauthored = count_moat_cells(&resident, &[], (0, 0), 100, 4, |x, _| x < 0);
        assert_eq!(unauthored, 9 * 9 - 1 - 4 * 9);
        // No resident set, no moat.
        assert_eq!(count_moat_cells(&HashSet::new(), &[], (0, 0), 100, 4, |_, _| false), 0);
    }

    #[test]
    fn observe_keys_matches_observe_for_churn() {
        let mut churn = ChurnTracker::default();
        let a: HashSet<(i32, i32, i32)> = [(4, 0, 0)].into_iter().collect();
        churn.observe_keys(a.clone());
        churn.observe_keys(HashSet::new());
        assert_eq!(churn.churned(), 0, "leaving is not churn until it returns");
        churn.observe_keys(a);
        assert_eq!(churn.churned(), 1);
    }

    #[test]
    fn resident_masks_mark_the_cells_a_finest_quad_holds() {
        let resident: HashSet<(i32, i32)> = [(4, 8), (5, 8), (7, 11), (0, 0), (9, 9)].into_iter().collect();
        // Quad (4, 8) covers x 4..=7, y 8..=11: cells (4,8) bit 0, (5,8) bit 1, (7,11) bit 15.
        assert_eq!(quad_resident_mask(4, 8, &resident), 0b1000_0000_0000_0011);
        assert_eq!(quad_resident_mask(100, 100, &resident), 0);
        let all: HashSet<(i32, i32)> = (0..4).flat_map(|y| (0..4).map(move |x| (x, y))).collect();
        assert_eq!(quad_resident_mask(0, 0, &all), 0xFFFF);
    }

    #[test]
    fn a_world_point_maps_to_its_cell_bit() {
        // Quad SW cell (-8, 4): x -32768..-16384, y 16384..32768.
        assert_eq!(quad_cell_bit(-8, 4, -32768.0 + 1.0, 16384.0 + 1.0), Some(1));
        assert_eq!(quad_cell_bit(-8, 4, -32768.0 + 4096.0 * 3.0 + 1.0, 16384.0 + 4096.0 * 3.0 + 1.0), Some(1 << 15));
        assert_eq!(quad_cell_bit(-8, 4, -32768.0 - 1.0, 16384.0 + 1.0), None, "west of the quad");
        assert_eq!(quad_cell_bit(-8, 4, -32768.0 + 4096.0 * 4.0, 16384.0), None, "east edge belongs to the next quad");
    }
}
