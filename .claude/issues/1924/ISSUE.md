# MAT-D6-01: Behavior-changing material-classifier edit shipped under a docs-titled commit (977eb95a)

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1924

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/core/src/ecs/components/material.rs:472-477` (the "scrap" arm in `classify_pbr_keyword`), introduced by commit `977eb95a` titled "Add Scripting Subsystem Audit report for 2026-07-06"
**Status**: NEW

## Description
`git show 977eb95a -- crates/core/src/ecs/components/material.rs` reveals a 43-line diff that adds a real classifier arm: any texture path containing "scrap" now returns `metalness 0.0, roughness 0.85` and is checked before the "metal" arm (so `metalscrap*` cladding no longer chromes). This is a genuine, all-game material-behavior change to the canonical classifier — not documentation — yet it ships under a commit message describing a scripting-audit report.

## Evidence
Diff confirmed; accompanying test `classify_pbr_scrap_metal_is_not_chrome` (material.rs:802) passes. The commit stat lists the material.rs change alongside shader binaries under a scripting-audit title.

## Impact
The code itself is correct and tested; the risk is purely archaeological — a material-classifier change hidden behind a docs commit defeats bisect/blame for anyone tracing a material regression to its origin.

## Related
User memory `chrome_flyer_pbr_classifier_gap`; the 2026-07-06 scrap-metal report

## Suggested Fix
Record the true content of `977eb95a` in HISTORY.md so the material change is discoverable; going forward, keep behavior changes out of report-titled commits.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr`, or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
