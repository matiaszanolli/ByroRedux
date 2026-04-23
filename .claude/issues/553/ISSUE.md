# NIF-08: NiFloatExtraData + NiFloatExtraDataController missing (SE: 1,492, cross-game: 1,657)

**Severity**: HIGH | **Dimension**: Coverage Gaps | **Game**: FO3, FNV, Skyrim SE | **Audit**: docs/audits/AUDIT_NIF_2026-04-22.md § NIF-08

## Summary
`NiFloatExtraData` is a generic float-valued metadata tag, widely used for tool-authored engine hooks (FOV multipliers, scale overrides, wetness levels). The controller subtype animates it. 1,312 + 180 blocks on SE; also present on FNV/FO3.

## Evidence
All three unknown sweeps.

## Location
`crates/nif/src/blocks/mod.rs:326-332` — NiExtraData arm covers String/Binary/Integer variants, not Float.

## Suggested fix
Extend the existing `NiExtraData::parse` enum-style multi-type match to accept `"NiFloatExtraData"` (identical in shape to `NiIntegerExtraData` but with `f32` payload). ~5 LOC + test.

## Completeness Checks
- [ ] **SIBLING**: NiFloatExtraDataController follows the same pattern — add both in one pass
- [ ] **TESTS**: Round-trip at FNV bsver
- [ ] **REAL-DATA**: All three unknown sweeps drop the float-extra-data bucket to 0

Fix with: /fix-issue <number>
