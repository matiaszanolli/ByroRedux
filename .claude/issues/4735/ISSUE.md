# EXT-D5-2026-09-21-04: WaterMaterial.scroll_* doc describes the pre-#4544 composition in the wrong units

**Issue**: #4735
**Filed**: 2026-09-22 (audit-publish, AUDIT_EXTERIOR_2026-09-21.md)

**Severity**: LOW (doc rot)
**Dimension**: Water translation (WATAL)
**Tier Violated**: no-fabrication (the contract text asserts a composition the code does not do)
**Game Affected**: all
**Location**: `crates/core/src/ecs/components/water.rs:254-258`

## Description
The doc says "(xy = m/s)" and "vector 1 is a perpendicular shear at half speed". Confirmed at HEAD `ee6d3fb39` the code disagrees on every point: the translate emits UV/s, vector 0 also carries the confined authored layer, #4544 made the shear 0.25x (not half), and the doc still says "the cell loader" where the translate now lives in `env_translate.rs`.

## Evidence
`byroredux/src/env_translate.rs:911-918`, `:533` (`WATER_CROSS_STREAM_SCROLL = 0.25`).

## Impact
This is the canonical-contract text that #4728 (EXT-D5-02)'s fix has to choose a convention against; right now it is wrong on units and composition.

## Suggested Fix
Rewrite the doc as part of the #4727/#4728 (EXT-D5-01/02) contract fix: units, composition, shear factor and sign convention.

## Related
#4544 (closed), #4727 (EXT-D5-01), #4728 (EXT-D5-02)

## Source
docs/audits/AUDIT_EXTERIOR_2026-09-21.md (EXT-D5-2026-09-21-04)
