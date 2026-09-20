# EXT-D6-2026-09-19-05: partition guards never run over a mixed-depth availability pattern

- **ID**: EXT-D6-2026-09-19-05
- **Labels**: low,terrain-exterior,bug,test-gap
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4504

**Severity**: LOW (test-gap; the partition property itself is structural and verified) · **Dimension**: Distant LOD · **Game Affected**: Skyrim, FO4, FO3, FNV
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D6-2026-09-19-05)

**Location**: `byroredux/src/cell_loader/lod_bands.rs:588-608` (`partition_never_covers_a_cell_twice`, all-available), `:1085-1107` (`coarsened_selection_is_still_a_partition`, level-8-only)

**Description**
Both overlap guards exercise only uniform availability patterns (everything baked / one band baked). Real worldspaces mix depths per quad (FO3's `washmontop` bakes 112 level-4 + 33 level-8 + 7 level-16 + 1 oddity; a 4/16/32 FO76-style ladder mixes three). The only mixed-pattern test (`memoising_the_availability_probe_preserves_the_selection`) asserts memo-equivalence, not the partition property. The property IS structural — `select_lod_quads`' descent is emit-OR-recurse, never both — and `lod_coverage::find_overlaps` audits live residency at runtime, so this is hardening, not a bug: a future edit that made emission conditional on something besides the recurse branch (e.g. a per-level cull) would pass every current partition guard.

**Suggested Fix**
One test running the cell-by-cell overlap check over a synthetic mixed pattern (e.g. "16 and 4 baked, 8 not, per-quad noise"), reusing the `partition_never_covers_a_cell_twice` loop.

## Completeness Checks
- [ ] **TESTS**: The mixed-pattern test is itself the fix
