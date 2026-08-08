# SF-D4-05: SECH/AOPF have zero dispatch, no typed capture or skip telemetry unlike sibling audio-metadata types

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2636
**Finding ID**: SF-D4-05

**Severity**: LOW
**Dimension**: 4 (Starfield ESM Resolve-Rate Baseline)
**Location**: `crates/plugin/src/esm/records/mod.rs:423-451`
**Status**: NEW

## Description
`SECH`/`AOPF` have zero dispatch — no typed capture, no skip telemetry,
unlike every sibling audio-metadata type. `SOUN`/`ASPC` are captured via
`dispatch_misc_stub_group` into typed `EsmIndex` collections; `SECH`
(`BGSSoundEcho`) and `AOPF` (`BGSAudioOcclusionPrimitive`) are absent from
every match arm and fall to the bare `_ => skip_group` catch-all — no
counter, no `skipped_unconsumed_groups` entry.

## Evidence
`citycydoniamainlevel` alone has 190 `SECH` + 30 `AOPF` REFRs (0.8% of the
cell). Both are genuine Starfield-era FourCCs (Gibbed `FormType.cs`
confirms), not garbage.

## Impact
No visible-content loss (neither type has a mesh) — purely diagnostic: a
future FourCC repurposing or content patch would be invisible.

## Suggested Fix
Add `b"SECH" | b"AOPF"` to the `dispatch_misc_stub_group` arm alongside
`SOUN`/`ASPC`.

## Related
#1568 (the precedent this should follow).

## Completeness Checks
- [ ] **TESTS**: A fixture with a `SECH`/`AOPF` group asserts it's captured via `dispatch_misc_stub_group`, not the catch-all
