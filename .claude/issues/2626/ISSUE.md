# SF-D9-2026-08-07-01: bgem_uses_glass_behavior treats raw refraction bit as unconditional glass signal

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2626
**Finding ID**: SF-D9-2026-08-07-01

**Severity**: MEDIUM
**Dimension**: 9 (BGSM/BGEM External Material Flow)
**Location**: `byroredux/src/asset_provider/material.rs:110-113` (`bgem_uses_glass_behavior`)
**Status**: NEW

## Description
`if bgem.glass_enabled || bgem.base.refraction { return true; }`.
`glass_enabled` is a v21+ field authored specifically to mean glass; the
careful v<21 feature-bundle heuristic below it (`hard_transparent_shell &&
reflective_surface_maps && lit_fresnel_falloff`) exists precisely because
the pre-v21 format has no such field. `base.refraction` is a different,
shared `BaseMaterial` screen-distortion bit — authored on heat shimmer,
cloaking shells, force-field ripple, fire/plasma distortion — none of which
are glass, and it is neither gated behind the alpha/decal/conductor guards
nor version-gated, so it fires on v2 through v22 alike.

## Evidence
```rust
// byroredux/src/asset_provider/material.rs:110-113
if bgem.glass_enabled || bgem.base.refraction { return true; }
```
`base.refraction` is a shared distortion bit, not glass-specific, and is
checked unconditionally regardless of BGEM version.

## Impact
`material.bgem_glass = true` and (since distortion cards are typically
`non_occluder`) `THIN_GLASS` too; in `helpers.rs:73-85`, `bgem_glass` makes
the mesh an `effect_glass_carrier`, one of the few things allowed to
*override* an already-selected engine-synthesized material kind — demoting
a correctly-classified effect-shader mesh to `MATERIAL_KIND_GLASS` and
stamping fixed metalness/roughness/IOR over its authored PBR. Same corpus
#2297 separately flags as `MATERIAL_KIND_FIRE_REFRACTION` content.

## Suggested Fix
Drop `|| bgem.base.refraction` from the short circuit, or fold it into the
v<21 bundle as one more conjunct. Add a regression fixture:
`refraction=true, effect_lighting_enabled=false`, no envmap stack — must
NOT classify glass.

## Related
#2297 (fire-refraction content this misclassification collides with).

## Completeness Checks
- [ ] **TESTS**: A `refraction=true`, no-envmap-stack fixture asserts `bgem_uses_glass_behavior() == false`
