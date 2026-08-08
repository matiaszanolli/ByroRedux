# FO4-D7-02: BGSM subsurface/rim-lighting suite parsed but never forwarded to canonical Material

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2607
**Finding ID**: FO4-D7-02

**Severity**: MEDIUM
**Dimension**: 7 (Canonical Material)
**Location**: `crates/bgsm/src/bgsm.rs:57-61` (parse), `merge_external_material` (`byroredux/src/asset_provider/material.rs:659`)
**Status**: NEW

## Description
BGSM's subsurface/rim-lighting suite (`rim_lighting`, `rim_power`,
`subsurface_lighting`, `subsurface_lighting_rolloff`) is parsed by
`crates/bgsm/src/bgsm.rs:57-61` but never forwarded by
`merge_external_material` onto `ImportedMaterial`, even though
`ImportedMaterial` has matching `subsurface_rolloff`/`rimlight_power` slots —
currently filled only from the NIF-native shader-property path (per #2284),
never from BGSM. `byroredux/src/render/static_meshes.rs` hardcodes zeros for
these regardless of what BGSM actually authored.

## Evidence
`crates/bgsm/src/bgsm.rs:57-61` parses `rim_lighting`/`rim_power`/
`subsurface_lighting`/`subsurface_lighting_rolloff`; `merge_external_material`
(`byroredux/src/asset_provider/material.rs:659`) does not forward any of
these onto `ImportedMaterial`'s matching `subsurface_rolloff`/
`rimlight_power` fields.

## Impact
Matters more than a typical dropped-field bug because `MAT_FLAG_PBR_BSDF` is
now unconditionally set for every BGSM material (#1352) — the Disney BSDF
subsurface/rimlight lobe is live in the shader, but every BGSM-authored
material feeds it hardcoded zeros regardless of what the artist actually
authored in the BGSM file.

## Suggested Fix
Forward BGSM's `rim_lighting`/`rim_power`/`subsurface_lighting`/
`subsurface_lighting_rolloff` onto `ImportedMaterial`'s
`rimlight_power`/`subsurface_rolloff` in `merge_external_material`, the same
way the NIF-native path already does per #2284.

## Related
FO4-D7-01, FO4-D7-03 (same BGSM-merge-boundary drop class); #2284
(NIF-native path for the same fields); #1352 (unconditional PBR_BSDF flag,
what makes this matter).

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Forward at the merge boundary (`merge_external_material`), not in the renderer
- [ ] **TESTS**: A regression test with a BGSM fixture carrying non-zero rim/subsurface values pins the forwarded fields
