# #5520: EXT-D6-2026-10-09-01: #5387's majority-residue prune breaks 3 of its 7 multi-residue ties on an arbitrary `Reverse(residue)`, and in two of them the dropped generation covers 2 authored cells the kept one does not

**Labels**: bug, game:fo3, low, terrain-exterior

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-09.md` — finding `EXT-D6-2026-10-09-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW. Two edge cells in each of two small worldspaces lose distant objects. The rule decides with no data in 3 of 7 cases.
- **Dimension**: Distant LOD and trees
- **Location**: `byroredux/src/cell_loader/lod_bands.rs:453-488` (`prune_same_level_overlaps`; the tie-break is at `:470`), called from `select_authored_lod_quads` at `:504`. Test: `same_level_leftover_generation_quads_are_pruned` (`:616`) has no tie case.
- **Status**: NEW. This is a follow-up of #5387 (closed); the fix is correct in every non-tie case.
- **Tier Violated**: no-fabrication. The tie-break is an arbitrary choice even though the selection could be anchored in data.
- **Game Affected**: Fallout 3 (FNV and both games' terrain tables are single-residue)
- **Description**:
  - The prune keeps the residue class with the most quads per level. Ties go to the smallest `(rx, ry)`, "for determinism".
  - Census of every FO3 object table with more than one residue class:
    - **washmontop**: the 13-quad majority (4,4) covers every authored cell that any class covers.
    - **dcworld01, dcworld17**: the majority covers every authored cell that any class covers.
    - **dcworld06 (3/3)**, **dcworld12 (4/4)** and **dlc02baileyscrossroads (7/7, Anchorage)**: exact ties, so the tie-break decides.
- **Evidence** (`tie_loss.py`: FO3 BSA name tables against the `XCLC` grids in `Fallout3.esm` / `Anchorage.esm`):

  | Worldspace | Kept residue → authored cells covered | Dropped residue → authored cells covered | Authored cells only the dropped set covers |
  |---|---|---|---|
  | dcworld06 (L8) | (1,5) → 69 | (1,6) → 71 | 2: (15,−3), (16,−3) |
  | dlc02baileyscrossroads (L4) | (0,2) → 67 | (0,3) → 69 | 2: (12,−8), (12,−7) |
  | dcworld12 (L8) | (2,6) → 87 | (4,6) → 87 | 0 |

- **Impact**:
  - Four edge cells across two worldspaces get no distant-object LOD.
  - The choice of which generation's geometry draws in these three worldspaces does not depend on the data.
  - Which lattice the shipped game itself loads is not established.
- **Related**: #5387, #5222 (closed); EXT-D6-2026-10-08-01.
- **Suggested Fix**:
  - Break count ties on authored-cell coverage. `wctx`'s exterior-cell set is already in hand at both ring call sites. Fall back to the residue order only when coverage also ties.
  - Add a tie fixture. Record in exal.md that which lattice the engine loads is still open.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
