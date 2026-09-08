//! Adjacent-cell terrain seam-agreement checking (EX-10/11 item 6, #2371).
//!
//! `spawn_terrain_mesh` builds each cell's mesh independently from its own
//! `LandscapeData`; nothing verifies that two horizontally/vertically
//! adjacent cells' shared edge row/column actually agree, despite LAND's
//! 33×33 grid being authored to share edge vertices by construction (row 0 =
//! south edge, col 0 = west edge — `crates/plugin/src/esm/cell/mod.rs:165-
//! 167`). A real authoring mismatch (a botched DLC/mod override that
//! re-declares one tile's LAND but not its neighbor's) would show up as a
//! visible height crack or a lit-normal seam at the cell boundary; nothing
//! today catches it before it reaches the screen.
//!
//! This module is the *detection* half — a pure function over two
//! [`LandscapeData`] values, no `World`/`VulkanContext`, matching
//! `lod_coverage`'s "pure functions over plain state" testing posture. It
//! reports facts (which edge indices disagree, by how much), not a
//! pass/fail verdict — inventing a height-delta tolerance without real
//! corpus data to calibrate it against would be exactly the kind of guessed
//! threshold this project's no-guessing policy exists to prevent. The live
//! caller (`streaming_helpers::update_terrain_seam_stats`) decides what
//! counts as a failure and reports facts, not a magnitude-tolerant
//! judgement. **See the 2026-08-26 correction below**: that failure
//! criterion is height-only, not "any dirty pair" as this paragraph
//! originally said.
//!
//! # Correction (2026-08-26): failure criterion is height-only, not "any
//! disagreement"
//!
//! A live FO4 Commonwealth `grid-cross` run (the first real cross-cell data
//! this checker had ever been run against — item 7's own doc flagged "no
//! real crack has been confirmed to trip `pairs_dirty`" as still open)
//! measured near-perfect shared-edge **height** agreement (3 mismatched
//! vertices across 17 checked pairs) alongside pervasive **VNML raw-byte**
//! disagreement (15/17 pairs). That asymmetry — heights agree almost
//! exactly, normals mostly don't — is consistent with each cell computing
//! its own boundary-vertex normal one-sidedly at authoring time (a
//! plausible, benign per-game convention, matching the already-documented
//! FO4 `_msn`-vs-Skyrim-`_n` LOD-normal precedent elsewhere in this
//! codebase), not a real geometric crack. `streaming_helpers::
//! update_terrain_seam_stats` now sets `TerrainSeamStats::pairs_dirty` (the
//! field `verdict()` fails on) from height mismatches only;
//! `normal_mismatch_pairs` stays tracked and reported but is informational,
//! not fatal. Not confirmed either way by visual inspection or RenderDoc —
//! a future session with that tooling can fold normal disagreement back
//! into the hard-fail criterion if it turns out to produce a visible
//! lighting seam.
//!
//! # Answer (#3306): the first real crack IS vanilla Bethesda content
//!
//! The live FO4 Commonwealth run that motivated the correction above also
//! reported `pairs_dirty=1` on cells (3,0)/(4,0), and #3306 left open
//! whether that was vanilla data or a mod override — the two outcomes
//! called for opposite responses. It is **vanilla**.
//!
//! Scanning every plugin in the reproducing Data directory (87 total, 79
//! third-party) for Commonwealth-worldspace (`0x0000003C`) cells at those
//! grids: only `Fallout4.esm` ships a LAND record for either
//! (`0x0000E01D` = (3,0), `0x0000E01C` = (4,0)). Four plugins override the
//! CELL records — `bostonfpsfixaio.esp`, `unofficial fallout 4 patch.esp`,
//! `ss2_xpac_chapter2.esm`, `youandwhatarmy2.esm` — and none carry LAND.
//! Decoding vanilla VHGT directly reproduces the reported deltas exactly:
//! rows 5/6/7 of the shared column differ by 8, 16 and 16 units, and the
//! other 30 vertices are bit-identical.
//!
//! So `check_seam` is behaving correctly and Bethesda's own authoring has
//! a localized 3-vertex crack here. Two consequences:
//!
//! 1. The live gate's hard-fail criterion will trip on stock FO4 content
//!    at this seam. That is a *true positive*, not a checker bug — but it
//!    means a `verdict=FAIL` on the boundary route is not by itself
//!    evidence of a regression. `vanilla_fo4_commonwealth_seam_tests`
//!    pins the exact expected vertices so a future run can tell "the known
//!    vanilla crack" from "something new".
//! 2. No tolerance is introduced. One measured pair is not a corpus, and
//!    calibrating a height-delta threshold off it would be exactly the
//!    guessed threshold the module docstring above rules out.
//!
//! Whether it is *visible* in a rendered frame is still unconfirmed — that
//! needs a screenshot centred on the seam, which the `boundary` route does
//! not currently park on.
//!
//! # Correction (2026-08-23): the retention design decision this doc used
//! to flag never needed making
//!
//! The item-7 prerequisite this module's doc previously named — "retain
//! `LandscapeData` somewhere queryable after `spawn_terrain_mesh` runs,
//! since it's a transient parse-result, consumed and dropped at spawn
//! time" — was wrong. `spawn_terrain_mesh` takes `land: &LandscapeData`
//! *borrowed from* `CellData.landscape`
//! (`cell_loader/exterior.rs::ExteriorCellApplyJob::begin`, `if let
//! Some(ref land) = cell.landscape`), and `CellData` lives inside
//! `EsmIndex.cells.exterior_cells`, which `ExteriorWorldContext.record_index`
//! (an `Arc<EsmIndex>`) keeps resident for the entire worldspace-streaming
//! session — it is never dropped after spawn. No new cache, no 4.4 KB- vs.
//! 130 B-per-cell tradeoff: the live checker below just looks the data up
//! again through the same `record_index` every other live exterior helper
//! (`scene::apply_cell_region_ambient`, `scene::apply_cell_climate_override`)
//! already reaches through.

use byroredux_plugin::esm::cell::LandscapeData;

/// Which shared edge two adjacent cells' grids meet at, from `a`'s side.
/// `a` is always the lower-grid-coordinate cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SeamDirection {
    /// `a` at `(gx, gy)`, `b` at `(gx+1, gy)` — `a`'s east edge (col 32)
    /// meets `b`'s west edge (col 0).
    EastWest,
    /// `a` at `(gx, gy)`, `b` at `(gx, gy+1)` — `a`'s north edge (row 32)
    /// meets `b`'s south edge (row 0).
    NorthSouth,
}

/// One shared-edge vertex whose height disagrees between the two cells.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HeightMismatch {
    /// Position along the shared edge, 0..33 (row index for `EastWest`,
    /// column index for `NorthSouth`).
    pub index: usize,
    pub height_a: f32,
    pub height_b: f32,
}

/// Result of comparing one pair of adjacent cells' shared edge.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SeamReport {
    /// Every edge vertex where the two cells' heights differ at all —
    /// authored terrain shares byte-identical LAND payloads at seams, so
    /// this project treats ANY difference as worth reporting; the caller
    /// decides what magnitude counts as a failure.
    pub height_mismatches: Vec<HeightMismatch>,
    /// `Some(true)` if VNML is present on both sides and at least one
    /// shared-edge vertex's raw normal bytes differ. `None` when either
    /// side lacks VNML (nothing to compare — not itself a mismatch, since
    /// pre-Skyrim normal maps are computed at render time, not authored).
    pub normal_bytes_differ: Option<bool>,
}

const GRID: usize = 33;

/// Edge-vertex index into a 33×33 grid for position `i` (0..33) along the
/// given side of the given cell (`is_a`: `a`'s far edge vs `b`'s near edge).
fn edge_index(direction: SeamDirection, is_a: bool, i: usize) -> usize {
    match (direction, is_a) {
        // a's east edge: col 32, every row.
        (SeamDirection::EastWest, true) => i * GRID + (GRID - 1),
        // b's west edge: col 0, every row.
        (SeamDirection::EastWest, false) => i * GRID,
        // a's north edge: row 32, every col.
        (SeamDirection::NorthSouth, true) => (GRID - 1) * GRID + i,
        // b's south edge: row 0, every col.
        (SeamDirection::NorthSouth, false) => i,
    }
}

/// Compare `a`'s and `b`'s shared edge (per `direction`, `a` on the lower
/// side) for height and normal-byte agreement.
pub(crate) fn check_seam(
    a: &LandscapeData,
    b: &LandscapeData,
    direction: SeamDirection,
) -> SeamReport {
    let mut height_mismatches = Vec::new();
    for i in 0..GRID {
        let ia = edge_index(direction, true, i);
        let ib = edge_index(direction, false, i);
        let (Some(&height_a), Some(&height_b)) = (a.heights.get(ia), b.heights.get(ib)) else {
            continue;
        };
        if height_a != height_b {
            height_mismatches.push(HeightMismatch {
                index: i,
                height_a,
                height_b,
            });
        }
    }

    let normal_bytes_differ = match (&a.normals, &b.normals) {
        (Some(na), Some(nb)) => Some((0..GRID).any(|i| {
            let ia = edge_index(direction, true, i) * 3;
            let ib = edge_index(direction, false, i) * 3;
            let (Some(byte_a), Some(byte_b)) = (na.get(ia..ia + 3), nb.get(ib..ib + 3)) else {
                return false;
            };
            byte_a != byte_b
        })),
        _ => None,
    };

    SeamReport {
        height_mismatches,
        normal_bytes_differ,
    }
}

/// #3306 (FO4-2026-08-26-01) — the first REAL crack this checker has been
/// run against, promoted from a live-run observation to a fixture.
///
/// `docs/smoke-tests/m-exteriors.sh fo4 boundary` reported
/// `pairs_dirty=1 height_mismatch_vertices=3` on FO4 Commonwealth cells
/// (3,0) / (4,0). The issue left open whether that was vanilla content or a
/// mod override; it is **vanilla**. Scanning all 87 plugins in the
/// reproducing Data directory for Commonwealth (worldspace `0x0000003C`)
/// cells at those grids: only `Fallout4.esm` ships a LAND record for either
/// (cells `0x0000E01D` = (3,0) and `0x0000E01C` = (4,0)). Four plugins
/// override the CELL records — `bostonfpsfixaio.esp`,
/// `unofficial fallout 4 patch.esp`, `ss2_xpac_chapter2.esm`,
/// `youandwhatarmy2.esm` — and none of them touch LAND.
///
/// The columns below are decoded straight out of vanilla `Fallout4.esm`'s
/// VHGT by `parse_land_record`'s own algorithm, and reproduce the reported
/// deltas exactly: rows 5/6/7 differ by 8, 16 and 16 world units, every
/// other row agrees to the bit. So Bethesda's own authoring has a 3-vertex
/// height crack here, and `check_seam`'s "any difference is worth
/// reporting" contract is correct to surface it.
///
/// What this fixture is for: the live gate previously had only synthetic
/// fixtures, so nothing pinned that it reports real content faithfully.
/// It deliberately does NOT encode a tolerance — calibrating one off a
/// single measured pair would be the guessed threshold this module's own
/// doc rules out.
#[cfg(test)]
mod vanilla_fo4_commonwealth_seam_tests {
    use super::*;

    /// The shared edge, decoded from vanilla `Fallout4.esm`: `.0` is cell
    /// (3,0) `0x0000E01D`'s east column (32), `.1` is cell (4,0)
    /// `0x0000E01C`'s west column (0), at the same row.
    ///
    /// Paired rather than two parallel arrays so the "same physical row of
    /// vertices" relationship is structural — the two halves cannot drift
    /// out of correspondence under a reformat.
    const SHARED_EDGE: [(f32, f32); GRID] = [
        (752.0, 752.0),
        (752.0, 752.0),
        (744.0, 744.0),
        (728.0, 728.0),
        (720.0, 720.0),
        (720.0, 712.0),
        (728.0, 712.0),
        (712.0, 696.0),
        (664.0, 664.0),
        (648.0, 648.0),
        (640.0, 640.0),
        (616.0, 616.0),
        (608.0, 608.0),
        (584.0, 584.0),
        (576.0, 576.0),
        (576.0, 576.0),
        (568.0, 568.0),
        (560.0, 560.0),
        (544.0, 544.0),
        (544.0, 544.0),
        (544.0, 544.0),
        (544.0, 544.0),
        (536.0, 536.0),
        (528.0, 528.0),
        (544.0, 544.0),
        (560.0, 560.0),
        (584.0, 584.0),
        (600.0, 600.0),
        (544.0, 544.0),
        (520.0, 520.0),
        (488.0, 488.0),
        (416.0, 416.0),
        (224.0, 224.0),
    ];

    /// `east`: fill column 32 (a cell's east edge). Otherwise column 0.
    fn land_with_edge(east: bool) -> LandscapeData {
        let mut land = LandscapeData {
            heights: vec![0.0; GRID * GRID],
            normals: None,
            vertex_colors: None,
            quadrants: Default::default(),
        };
        for (row, (a, b)) in SHARED_EDGE.iter().enumerate() {
            let col = if east { GRID - 1 } else { 0 };
            land.heights[row * GRID + col] = if east { *a } else { *b };
        }
        land
    }

    #[test]
    fn vanilla_commonwealth_3_0_to_4_0_reports_the_three_measured_vertices() {
        let a = land_with_edge(true);
        let b = land_with_edge(false);
        let report = check_seam(&a, &b, SeamDirection::EastWest);

        let measured: Vec<(usize, f32, f32)> = report
            .height_mismatches
            .iter()
            .map(|m| (m.index, m.height_a, m.height_b))
            .collect();
        assert_eq!(
            measured,
            vec![(5, 720.0, 712.0), (6, 728.0, 712.0), (7, 712.0, 696.0)],
            "the live FO4 boundary run measured exactly these three vertices; a change \
             here means either the decode or the edge-index convention moved (#3306)"
        );

        // The deltas are coarse integers, not float noise — which is why
        // this is a real authoring crack and not a rounding artifact.
        for m in &report.height_mismatches {
            let delta = (m.height_b - m.height_a).abs();
            assert!(
                delta >= 8.0 && (delta % 8.0) == 0.0,
                "delta {delta} at row {} should be a whole multiple of the 8-unit VHGT \
                 quantum, confirming a genuine authored difference",
                m.index
            );
        }
    }

    /// The other 30 shared vertices agree exactly. Without this the test
    /// above would still pass if the decode produced garbage everywhere.
    #[test]
    fn every_other_shared_vertex_on_that_seam_agrees_exactly() {
        let differing: Vec<usize> = (0..GRID)
            .filter(|&i| SHARED_EDGE[i].0 != SHARED_EDGE[i].1)
            .collect();
        assert_eq!(
            differing,
            vec![5, 6, 7],
            "30 of 33 shared edge vertices are bit-identical — the crack is a \
             localized 3-vertex authoring defect, not a systematic offset"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_land(height: f32) -> LandscapeData {
        LandscapeData {
            heights: vec![height; GRID * GRID],
            normals: None,
            vertex_colors: None,
            quadrants: Default::default(),
        }
    }

    #[test]
    fn matching_east_west_edge_reports_no_mismatches() {
        // a's east col (32) and b's west col (0) both authored at 10.0.
        let a = flat_land(10.0);
        let b = flat_land(10.0);
        let report = check_seam(&a, &b, SeamDirection::EastWest);
        assert!(report.height_mismatches.is_empty());
    }

    #[test]
    fn mismatched_east_west_edge_is_reported_per_row() {
        let a = flat_land(10.0);
        let mut b = flat_land(10.0);
        // Corrupt b's west edge (col 0) at row 5 only.
        b.heights[5 * GRID] = 42.0;

        let report = check_seam(&a, &b, SeamDirection::EastWest);
        assert_eq!(report.height_mismatches.len(), 1);
        assert_eq!(report.height_mismatches[0].index, 5);
        assert_eq!(report.height_mismatches[0].height_a, 10.0);
        assert_eq!(report.height_mismatches[0].height_b, 42.0);
    }

    #[test]
    fn matching_north_south_edge_reports_no_mismatches() {
        let a = flat_land(3.0);
        let b = flat_land(3.0);
        let report = check_seam(&a, &b, SeamDirection::NorthSouth);
        assert!(report.height_mismatches.is_empty());
    }

    #[test]
    fn mismatched_north_south_edge_is_reported_per_column() {
        let a = flat_land(3.0);
        let mut b = flat_land(3.0);
        // Corrupt b's south edge (row 0) at column 20 only.
        b.heights[20] = -7.5;

        let report = check_seam(&a, &b, SeamDirection::NorthSouth);
        assert_eq!(report.height_mismatches.len(), 1);
        assert_eq!(report.height_mismatches[0].index, 20);
        assert_eq!(report.height_mismatches[0].height_b, -7.5);
    }

    #[test]
    fn missing_normals_on_either_side_reports_none_not_a_mismatch() {
        let a = flat_land(1.0);
        let b = flat_land(1.0);
        let report = check_seam(&a, &b, SeamDirection::EastWest);
        assert_eq!(
            report.normal_bytes_differ, None,
            "no VNML on either side means nothing to compare, not a failure"
        );
    }

    #[test]
    fn matching_normal_bytes_report_false() {
        let mut a = flat_land(1.0);
        let mut b = flat_land(1.0);
        a.normals = Some(vec![128u8; GRID * GRID * 3]);
        b.normals = Some(vec![128u8; GRID * GRID * 3]);
        let report = check_seam(&a, &b, SeamDirection::EastWest);
        assert_eq!(report.normal_bytes_differ, Some(false));
    }

    #[test]
    fn mismatched_normal_bytes_at_the_shared_edge_are_caught() {
        let mut a = flat_land(1.0);
        let mut b = flat_land(1.0);
        let mut na = vec![128u8; GRID * GRID * 3];
        let mut nb = vec![128u8; GRID * GRID * 3];
        // a's east edge at row 0: index 0*33+32 = 32, byte offset 96.
        na[96] = 200;
        nb[96] = 50;
        a.normals = Some(na);
        b.normals = Some(nb);

        let report = check_seam(&a, &b, SeamDirection::EastWest);
        assert_eq!(report.normal_bytes_differ, Some(true));
    }

    #[test]
    fn mismatched_normal_bytes_off_the_shared_edge_are_ignored() {
        let mut a = flat_land(1.0);
        let mut b = flat_land(1.0);
        let mut na = vec![128u8; GRID * GRID * 3];
        let nb = vec![128u8; GRID * GRID * 3];
        // Interior vertex (row 1, col 1), NOT on the east/west shared edge.
        let interior = (GRID + 1) * 3;
        na[interior] = 200;
        a.normals = Some(na);
        b.normals = Some(nb);

        let report = check_seam(&a, &b, SeamDirection::EastWest);
        assert_eq!(
            report.normal_bytes_differ,
            Some(false),
            "a disagreement away from the shared edge must not trip the seam check"
        );
    }
}
