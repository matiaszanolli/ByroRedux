# #5387: EXT-D6-2026-10-08-01: #5222's authored-quad selection draws overlapping stale FO3 object-LOD quads — `select_authored_lod_quads` assumes a level tiles, but vanilla ships same-level quads whose footprints and geometry overlap

**Labels**: medium,terrain-exterior,bug,game:fo3
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5387

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-08.md` — `EXT-D6-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: #5222 is closed; this is the remainder its fix (`b7987d813`) left (intra-level overlap), not a duplicate.

- **Severity**: MEDIUM. Duplicate distant geometry (z-fight, double cost) on shipped worldspaces.
- **Dimension**: Distant LOD and trees
- **Location**:
  - `byroredux/src/cell_loader/lod_bands.rs:441-496` (`select_authored_lod_quads`). Doc line 441: "Assumes a level's authored quads tile the ground they cover". There is no intra-level overlap check; the only suppression (`:482`) is against the immediately coarser level.
  - `byroredux/src/cell_loader/object_lod.rs:200-237` (the FalloutLegacyBlocks arm and the footprint-based `quad_intersects_full_detail` retain).
- **Status**: NEW. It was introduced by `b7987d813`; #5222 is closed and its fix is incomplete.
- **Tier Violated**: n/a
- **Game Affected**: Fallout 3. FNV census: 0 overlaps. Terrain diffuse quads: 0 overlaps in both games.
- **Description**:
  - The index enumerates every plain quad file in the archive name tables.
  - In six FO3 worldspaces, a single level holds quads on several residues whose 8- or 4-cell footprints overlap. They look like leftovers from different LOD generation runs.
  - With no finer level (washmontop is level-8 only), `any_finer` is false and every in-ring quad is selected. All overlapping blocks are drawn.
  - The geometry overlaps too, not just the names:
    - Washmontop blocks `x12.y-25` / `x12.y-22` / `x12.y-20` all reach the same far corner (world X ≈ 82,755, Y = −82,631 BU).
    - Their triangle counts nest (3,696 / 6,694 / 9,078).
    - `y-20`'s geometry also runs past its footprint (to cell y ≈ −11.6 vs the footprint edge −12), so the footprint-based full-detail retain can miss it.
- **Evidence** (FO3 BSA name tables; pairs of same-level quads with overlapping footprints):

  | Worldspace | Level | Quads | Overlapping pairs | Residues |
  |---|---|---|---|---|
  | washmontop (The Washington Monument) | 8 | 37 | 65 | (4,2), (4,4), (4,7) |
  | dcworld12 (Seward Square) | 8 | 8 | 6 | |
  | dcworld17 (Falls Church) | 8 | 6 | 5 | |
  | dcworld06 (Vernon Square) | 8 | 6 | 4 | |
  | dcworld01 (Chevy Chase) | 8 | 3 | 2 | |
  | dlc02baileyscrossroads | 4 | 14 | 11 | |
  | tlandscape (test world) | 4 | 48 | 168 | |

  - The unit tests use synthetic tilings. `washmontop_level8_spans_multiple_residues_and_all_select` asserts only that each quad is selected when standing in it, and never checks for overlap.
- **Impact**:
  - Duplicated, z-fighting distant buildings at the Washington Monument top, the vista worldspace where LOD matters most, and in four DC worldspaces.
  - The double draws also add GPU cost.
  - The live visual verification of #5222 is still owed, so no gate has seen this.
- **Related**: #5222 (closed, incomplete); #3502; #4468 (`.high.` variants, handled correctly).
- **Suggested Fix**:
  - Resolve same-level overlaps before selection. For example, per (worldspace, level), keep the largest mutually non-overlapping subset, or the majority residue class. Alternatively, prefer the block whose geometry bounds fit its own footprint.
  - Add a real-data test asserting that no two selected same-level quads overlap on washmontop.

## Completeness Checks
- [ ] **SIBLING**: Same overlap check applied to the terrain-diffuse authored quad path and the FNV index (census says 0 there today)
- [ ] **TESTS**: A regression test pins this specific fix
