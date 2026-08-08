# OBL-D5-03: resolve_pbr's classifier backstop hardcodes specular_authored: false, diverging from the real Oblivion signal

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2573
**Finding ID**: OBL-D5-03

**Severity**: LOW
**Dimension**: NIFAL Canonical Material Translation for Oblivion
**Location**: `crates/core/src/ecs/components/material.rs:815-829`
**Status**: NEW

## Description
The importer already carries the exact signal (`MaterialInfo::specular_authored`, true for every Oblivion mesh with a `NiMaterialProperty`) but it is never forwarded onto `ImportedMaterial`. `resolve_pbr`'s classifier backstop hardcodes `specular_authored: false`, diverging from the real Oblivion signal. Impact today is nil — the backstop is unreachable on the Oblivion path since overrides always arrive `Some` (per OBL-D5-01's confirmation, this session) — but becomes a live divergence the moment any future non-pre-classified source reaches `translate_material`, which is the same shape as the closed #1873 chrome-flyer regression.

## Evidence
Confirmed directly: `material.rs:825` — `specular_authored: false,` with a comment acknowledging "This backstop path... has no way to know whether `specular_color` was ever authored on this `Material` — assume not."

## Impact
None today (unreachable on the Oblivion path). Becomes a live divergence the moment any future non-pre-classified source reaches `translate_material`.

## Related
#1873 (chrome-flyer regression — same shape: a struct-default assumption masquerading as "not authored").

## Suggested Fix
Forward `specular_authored` onto `ImportedMaterial`, or delete the backstop arm entirely since every live producer already supplies `Some`.

## Completeness Checks
- [ ] **TESTS**: A regression test confirms `specular_authored` is forwarded correctly (or the backstop's removal doesn't break any live producer)
