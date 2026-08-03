# FO3-D5-04: FO3 collision baselines only sample Fallout - Meshes.bsa — three DLC-only collision block types are ungated

Filed from: `docs/audits/AUDIT_FO3_2026-08-03.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2334

**Severity**: LOW
**Location**: `crates/nif/tests/common/mod.rs:110`; consumed by `per_block_baselines.rs`/`block_coverage_baselines.rs`
**Status**: NEW

### Description
FO3 ships 5 DLC mesh archives with collision types absent from the sampled baseline archive: `bhkSPCollisionObject` (25), `bhkBlendCollisionObject` (171), `bhkConvexTransformShape` (38) — all parse cleanly (0 unknown) but no checked-in baseline pins their parsed/unknown split.

Confirmed against current code: `Game::mesh_archive()` (`crates/nif/tests/common/mod.rs`) returns exactly one hardcoded mesh-archive filename per game (e.g. `Fallout3 => "Fallout - Meshes.bsa"`) with no DLC archive enumeration, confirming the baseline sampling gap.

### Impact
A regression in the blend-collision-object tail or `bhkConvexTransformShape` decode would not be caught by any FO3 gate.

### Suggested Fix
Extend the FO3 (and FNV) test archive list to a `mesh_archives()` set covering all DLC, regenerate + check in baselines.

### Related
FO3-D5-02

## Completeness Checks
- [ ] **SIBLING**: Same single-archive sampling gap likely applies to FNV and other multi-DLC games
- [ ] **TESTS**: New/regenerated baseline TSVs pin the DLC-only collision block counts
