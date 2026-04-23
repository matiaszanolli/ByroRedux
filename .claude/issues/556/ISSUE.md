# NIF-11: NiBSBoneLODController + BSBoneLODExtraData missing (108 blocks)

**Severity**: MEDIUM | **Dimension**: Coverage Gaps | **Game**: FO3, FNV, Skyrim SE | **Audit**: docs/audits/AUDIT_NIF_2026-04-22.md § NIF-11

## Summary
`NiBSBoneLODController` + `BSBoneLODExtraData` pair drive per-skeleton bone-LOD: distance-based simplification where far-away NPCs skip fingers/spine-tip bones. 108 blocks across three games, mostly on character skeletons.

## Evidence
Combined count across FO3/FNV/Skyrim SE unknown sweeps.

## Location
`crates/nif/src/blocks/mod.rs` — no dispatch arms for either type.

## Suggested fix
Both types per nif.xml. Controller is a `NiTimeController` subclass with a `lod_levels` array; ExtraData is a simple LOD-distance list. ~40 LOC combined + dispatch arms.

## Completeness Checks
- [ ] **TESTS**: Round-trip both structs
- [ ] **REAL-DATA**: Three sweeps drop both buckets

Fix with: /fix-issue <number>
