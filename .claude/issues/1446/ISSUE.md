# #1446 — LC-D2-01: Stale doc-comments claim FO4 CSG path is deferred

_Snapshot as filed (2026-06-02) from AUDIT_LEGACY_COMPAT_2026-06-02.md. GitHub is authoritative for live state._

- **Severity**: LOW
- **Dimension**: D2 (NIF Format Readiness)
- **Location**: `byroredux/src/cell_loader/load.rs:231-239`; `byroredux/src/cell_loader/precombined.rs:11-33`
- **Status**: NEW

## Description
Block comments still say the FO4 CSG companion reader is deferred — `load.rs`: "a companion `Fallout4 - Geometry.csg` blob we don't yet parse, so spawning currently yields zero entities"; `precombined.rs`: "Until a CSG reader lands, this pass spawns zero entities" and "Deferred (future PreCombined-Geometry milestone): CSG / PSG companion reader". This contradicts the now-landed M49 path (commits b93ad7a9 → 2900de70). Logic is correct; only the comments are stale.

## Impact
Documentation rot; misleads the next reader into thinking precombined geometry is unrendered. No runtime effect.

## Suggested Fix
Refresh both comment blocks when #1351 is closed.

## Related
#1351 (resolved by M49).

## Completeness Checks
- [ ] **SIBLING**: Check other M49-era comments referencing "deferred CSG" / "spawns zero entities"
- [ ] **TESTS**: n/a (doc-only)

_Filed from [docs/audits/AUDIT_LEGACY_COMPAT_2026-06-02.md](../blob/main/docs/audits/AUDIT_LEGACY_COMPAT_2026-06-02.md)_
