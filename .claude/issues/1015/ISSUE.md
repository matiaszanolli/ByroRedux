# Issue #1015

**Title**: F-WAT-02: Water refraction misses paint sky tint UNDER the surface — should be fog/deep

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — F-WAT-02
**Severity**: HIGH
**File**: `crates/renderer/shaders/water.frag:177-206, 370-374`

## Premise verified (current `main`)

```glsl
// water.frag:190 (inside traceWaterRay, miss branch)
return skyTint.xyz;
// water.frag:205 (post-loop fallback)
return mix(skyTint.xyz, vec3(0.65, 0.7, 0.75), 0.4);
// water.frag:370 (refraction call site)
vec3 hitColor = traceWaterRay(vWorldPos, Tdir, REFRACTION_MAX_DIST, refrDist, refrHit);
```

`traceWaterRay` returns `skyTint.xyz` on miss for *both* reflection and refraction calls. Refraction misses are reachable at cell edges, over caves, or with sparse exterior BLAS.

## Issue

When a downward refraction ray escapes the BLAS, the surface radiance going in is sky-blue rather than the cell's fog/deep colour. `absorbWaterColumn` then mixes `deep_color` based on `hitDist = maxDist`, so deep tint dominates BUT the visible artefact is a faint sky glow instead of murk when the player looks straight down at shallow water near a cliff. Audit-checklist item #5 explicitly demands "missed rays sample backdrop with proper fog."

## Fix

Split `traceWaterRay` into reflection/refraction helpers, or take a `fallbackOnMiss: vec3` parameter. Reflection wants `skyTint`; refraction wants `push.deep.rgb` (or camera UBO `fog.rgb`).

## Test

Synthetic scene with no opaque geometry below a water plane should render uniform deep/fog tint, not sky.

## Completeness Checks

- [ ] **UNSAFE**: N/A — GLSL
- [ ] **SIBLING**: Verify reflection-miss skyTint behaviour preserved
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Cliff-edge water synthetic scene regression

