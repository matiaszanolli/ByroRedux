# OBL-D4-04: MAT_FLAG_PBR_BSDF verified always-0 for Oblivion, but is_pbr has no negative test pinning that invariant

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2570
**Finding ID**: OBL-D4-04

**Severity**: LOW
**Dimension**: Rendering Path for Oblivion Shaders
**Location**: `byroredux/src/cell_loader.rs:229-232`, `byroredux/src/asset_provider/material.rs:719,807,990,1148`, `crates/nif/src/import/material/mod.rs:1244`
**Status**: NEW (verified correct today — no defect, but a test-coverage gap)

## Description
Every `is_pbr = true` writer sits behind an external material file merge (`.mat`/BGSM/BGEM); the NIF import path hard-writes `is_pbr: false`, and Oblivion authors no BGSM/BGEM/.mat, so the Disney lobe is correctly unreachable. Risk is regression, not present behaviour: a future "promote legacy specular to PBR" heuristic could silently flip every Oblivion surface onto the Disney lobe, with no test to catch it.

## Evidence
Confirmed directly: `crates/nif/src/import/material/mod.rs:1244` — `is_pbr: false,` hardcoded on the NIF import path.

## Impact
None today. Risk is a silent future regression with no test-level tripwire.

## Suggested Fix
Add a one-line regression test asserting `NiMaterialProperty`+`NiTexturingProperty`-only `MaterialInfo` yields `is_pbr == false`.

## Completeness Checks
- [ ] **TESTS**: New regression test pins `is_pbr == false` for legacy-only (no BGSM/BGEM/.mat) material input
