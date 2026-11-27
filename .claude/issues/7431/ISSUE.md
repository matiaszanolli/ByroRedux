# #7431

**DLOD: FO3 drops whole .dtl tree files at the resident boundary, and the gate counts no tree moat for Oblivion or FO3**

State: OPEN · Labels: bug, medium, game:fo3, game:oblivion, terrain-exterior

Follow-up to #6633. PRD `docs/prd/distant-lod-completion.md` §3.2 (family F9). Found by code reading on 2026-11-27 (HEAD `d1540bb85`); nothing was run.

## Problem

1. **FO3 drops a whole `.dtl` file at the resident boundary.** `byroredux/src/cell_loader/tree_lod_legacy.rs` filters out any `.dtl` quad for which `footprint_has_resident_cell` is true, so its trees in non-resident cells are not drawn either. Each `.dtl` record carries a position, so the data does not force this. Skyrim's `.btt` ring already draws a straddling quad minus its resident cells (#6639). DLOD-TREE-008 (SHOULD) is unmet on FO3, and so are DLOD-G-001 and the Phase 2 coverage exit. The owner's decision Q8 (Gapped by data) does not cover this family.
2. **The gate cannot see it.** In `update_lod_coverage` (`byroredux/src/streaming_helpers.rs`), `tree_moat_cells` is computed only from the `.btt` ring, and `tree_load_cells` returns `None` for every title but Skyrim. It therefore reads 0 on Oblivion and FO3 whatever is drawn. Oblivion's ring is wanted per `.lod` cell, so no moat is expected there, but that has not been measured. DLOD-GATE-005 is met for object quads and Skyrim's trees only.

## Constraint

The existing hole masks do not carry over as they are. `quad_resident_mask` is a `u16` over a 4x4 quad, and `find_terrain_full_detail_overlaps` honours a mask only when `level == 4`. The PRD's census puts most vanilla `.dtl` files at level 8. The legacy tree keys are also passed to the overlap check with a mask of 0, so drawing part of a straddling file would count as an overlap until that check can see which cells were left out. FO3 trees spawn as one instanced entity per placement (`spawn_placement_groups`), so filtering placements by cell is one possible route.

## Acceptance Criteria

- [ ] DLOD-TREE-008: on FO3, a `.dtl` file whose footprint touches resident cells still draws the trees that stand in its non-resident cells
- [ ] DLOD-TREE-007 / DLOD-GATE-003: no FO3 tree is drawn in a resident full-detail cell, and `full_detail_overlaps` reads 0
- [ ] DLOD-GATE-005: `tree_moat_cells` is computed for the Oblivion and FO3 tree rings, and a test shows it reads nonzero when an authored tree cell beside the resident set is left uncovered
- [ ] DLOD-GATE-004: `tree_churn` reads 0 on a settled traversal
- [ ] DLOD-G-002: `tree_moat_cells=0` on `m-exteriors.sh oblivion boundary` and on an FO3 Wasteland run that has resident `.dtl` trees (A5's pose and radius; the `fo3` boundary profile sits in MegatonWorld, where the census found no `.dtl`)
- [ ] DLOD-G-001: PRD §3.2 and the ROADMAP FO3 and Oblivion rows are updated with the measured result

