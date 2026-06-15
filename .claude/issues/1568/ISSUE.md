**Severity**: MEDIUM · **Dimension**: SF ESM Resolve-Rate
**Location**: `crates/plugin/src/esm/records/mod.rs:223-300` (no `b"PDCL"` arm; falls through to warn-once/skip default)
**Source**: `docs/audits/AUDIT_STARFIELD_2026-06-14.md` (SF-D4-02)

## Description
`PDCL` (Gibbed `FormType.cs`: `BGSProjectedDecal`, 0x4C434450) is a Starfield-new base type with no dispatch arm and no consumer. It is the **single most frequent unresolved base type in Cydonia** (1 846 REFRs / 67 distinct forms — 59% of the unresolved count). Decals are projected onto surrounding geometry, so they would never enter the `statics` MODL path even if dispatched; a real decal-projection system would be needed to consume them.

## Evidence
Classifier output `PDCL 1846 REFRs 67 distinct forms` unresolved in `citycydoniamainlevel`; `grep b"PDCL"` over `crates/plugin/src/esm/` returns zero hits (confirmed). Gibbed `FormType.cs`: `PDCL = 0x4C434450`.

## Impact
All grime/blood/poster/signage decals in Cydonia absent. Cosmetic-only (no missing collision or structural geometry) and no visible garbage (silent skip), so MEDIUM. Numerically the biggest unresolved bucket but lowest structural impact.

## Related
SF-D4-04 (silent-skip behaviour); the `Decal` marker in `byroredux/src/components.rs`.

## Suggested Fix
Defer the real consumer until a decal-projection system exists. In the interim add a warned-once skip arm (the `warned_scol`/`warned_movs` pattern) so PDCL stops inflating the silent-skip count and is visible in telemetry. Do NOT route PDCL into `statics` — it has no MODL.

## Completeness Checks
- [ ] **SIBLING**: The warned-once arm matches the existing `warned_scol`/`warned_movs` skip pattern (one set, no per-REFR spam)
- [ ] **TESTS**: A test asserts a PDCL REFR is consciously skipped (not silently dropped) and counted in telemetry
