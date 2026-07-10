# CORN-D21-01: Cornell-box RT harness exercises only point lights — cannot bisect the sun-direction convention bugs found elsewhere in this audit

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1942

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `byroredux/src/cornell.rs:59-78` (`setup_cornell_scene`, `CellLightingRes` insert)
**Status**: NEW

## Description
The harness is described as a lighting-bisection reference but exercises only point lights. The inserted `CellLightingRes` zeroes the directional term (`directional_color: [0,0,0]`, `is_interior: true`) and the scene never inserts a `SkyParamsRes`. Consequently the sun/directional shading path, the volumetric froxel sun-injection path, and the Effect_Lit sun-shading path are all inert in Cornell — exactly the paths where this same audit found two real sun-direction sign bugs (VOL-D16-01, SKY-D18-01).

## Evidence
`grep -n "insert_resource|SkyParamsRes" cornell.rs` → single `insert_resource(CellLightingRes{...})`, no `SkyParamsRes`. `directional_color` is `[0,0,0]`.

## Impact
The harness cannot be used to bisect any sun-direction, directional-shadow, or volumetric-sun regression. A future dev reaching for `--cornell` to isolate a "sun looks wrong" regression would get a false all-clear because the sun path is never driven.

## Related
VOL-D16-01 (volumetrics sun-direction bug); SKY-D18-01 (Effect_Lit sun shading bug)

## Suggested Fix
Add an optional exterior variant (e.g. `--cornell-sun`) that inserts a `SkyParamsRes` with a non-zero directional light and a known-convention `sun_direction`. At minimum, document in the module header that the current harness is interior/point-light-only.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
