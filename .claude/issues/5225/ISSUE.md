# #5225 — FO3-D3-02: LOD census figures and one readiness-plan claim are stale or miscounted (doc-rot bundle)

https://github.com/matiaszanolli/ByroRedux/issues/5225

Source: `docs/audits/AUDIT_FO3_2026-10-03.md` (HEAD `be3cd9468`)

- **Severity**: LOW
- **Dimension**: Cell Loading (docs)
- **Location**:
  - `docs/engine/exterior-readiness-plan.md:449`;
  - `byroredux/src/cell_loader/object_lod.rs:192, 677, 688-703`;
  - `byroredux/src/cell_loader/lod_bands.rs:295`;
  - `.claude/commands/audit-fo3/SKILL.md` (Dim 3 LOD line, Game Context `.high.` line).
- **Status**: NEW
- **Description**:
  1. The readiness plan says "Oblivion/FO3/FNV correctly stay single-ring (no baked quadtree exists for those games)". That has been false since #3203 and #3321.
  2. "93 of 422 quads" counts file variants, not quads. The 422 `blocks\` entries in `Fallout - Meshes.bsa` include 54 `.high.` variants and 2 `…postapocalypse.nif`. The plain quads number 366, of which 65 are level-8-only.
  3. The #4468 note puts all FO3 `.high.` variants in Anchorage. The base archive also ships 54 (28 washmontop L8, 26 wasteland L4). Every one has a plain sibling, so the no-hole conclusion stands.
- **Evidence**: BSA name-table walk: `{'washmontop.levelN.high…': 28, 'wasteland.levelN.high…': 26, '…postapocalypse.nif': 2}`. Plain quads: base 366, DLC 331.
- **Impact**: auditors and fixers re-derive the wrong denominator, and the readiness plan tells readers the legacy games have no LOD quadtree.
- **Related**: FO3-D3-01, #3502, #4468.
- **Suggested Fix**:
  - Correct the readiness-plan line.
  - Restate the counts as plain quads (65 of 366 base; 697 corpus-wide).
  - Add the base-archive `.high.` count.
  - Update the `/audit-fo3` Dim 3 line once FO3-D3-01 lands.

## Completeness Checks
- [ ] **SIBLING**: Every "93 of 422" occurrence (`object_lod.rs`, `lod_bands.rs`) and the `/audit-fo3` skill lines are updated together
- [ ] **TESTS**: `.claude/commands/_audit-validate.sh` passes after the skill edit
