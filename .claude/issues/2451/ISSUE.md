# EXAL-03: CELL XCCM per-cell climate override is parsed with zero consumers

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2451
**Finding ID**: EXAL-03 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 5 — EXAL
**Location**: `crates/plugin/src/esm/cell/wrld.rs:385`, `walkers.rs:318`, `mod.rs:240`; `byroredux/src/scene/world_setup.rs:240`
**Status**: NEW (sub-finding under #2373)

## Description
`CellRecord::climate_override` (XCCM) parsed on both CELL walk paths, asserted in tests, never read. No per-cell weather re-resolve hook exists at all.

## Suggested Fix
Wire a per-cell weather re-resolve at the boundary, or document as a deliberate non-goal in exal.md §2.

## Related
#2373 (OPEN).

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Any per-cell override lives at the `env_translate.rs` boundary, not a new render-time branch
- [ ] **TESTS**: A regression test confirms a cell with XCCM set resolves to the overridden climate, not the worldspace default
