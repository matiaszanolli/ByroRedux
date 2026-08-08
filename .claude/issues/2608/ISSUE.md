# FO4-D7-03: BGSM environment_mapping_mask_scale parsed and dropped at merge boundary

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2608
**Finding ID**: FO4-D7-03

**Severity**: LOW
**Dimension**: 7 (Canonical Material)
**Location**: `crates/bgsm/src/base.rs:120-121`, `merge_external_material` (`byroredux/src/asset_provider/material.rs:659`)
**Status**: NEW

## Description
BGSM's `environment_mapping_mask_scale` is parsed
(`crates/bgsm/src/base.rs:120-121`) but dropped at the merge boundary —
`merge_external_material` forwards env-map *textures* but not the scale
value. Same drop-at-boundary class as FO4-D7-02.

## Evidence
`crates/bgsm/src/base.rs:120-121` parses `environment_mapping_mask_scale`;
`merge_external_material` (`byroredux/src/asset_provider/material.rs:659`)
forwards the env-map texture reference but not this scale field.

## Impact
BGSM-authored env-map mask scaling is silently ignored — env-map reflection
intensity renders at an implicit default rather than the artist-authored
scale.

## Suggested Fix
Forward `environment_mapping_mask_scale` onto the matching
`ImportedMaterial` field in `merge_external_material`.

## Related
FO4-D7-02 (same class of dropped BGSM field at the merge boundary).

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Forward at the merge boundary, not in the renderer
- [ ] **TESTS**: A regression test with a non-default `environment_mapping_mask_scale` BGSM fixture pins the forwarded value
