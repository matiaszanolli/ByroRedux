# Issue #1014

**Title**: F-WAT-01: Water refract() called with wrong incident-vector sign — refraction ray fires upward

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — F-WAT-01
**Severity**: HIGH
**File**: `crates/renderer/shaders/water.frag:365`

## Premise verified (current `main`)

```glsl
// water.frag:291
vec3 V = normalize(cameraPos.xyz - vWorldPos);
// water.frag:365
vec3 Tdir = refract(-V, Nperturbed, 1.0 / max(ior, 1.0));
```

`V` is fragment→camera (line 291). GLSL `refract(I, N, eta)` wants `I` as the **incident** vector (camera→fragment), with `N` pointing **against** I. Passing `-V` flips back to fragment→camera — wrong direction for a downward-entering ray. Refracted ray fires *upward into the air column* instead of downward through the water.

## Issue

Reflection on line 351 (`reflect(-V, Nperturbed)`) is geometrically symmetric so it survives the same flip; refraction does not. Visible artefact when refraction misses (cliff edges, sparse BLAS) or when compared to ground-truth — a regression test would catch it via `assert_almost_eq` on a 45° incidence (`tan(θ_t) ≈ 0.564` for IOR=1.33).

## Fix

Pass actual incident ray per GLSL convention. `V` is fragment→camera, so the incident is `-V` (camera→fragment); the refracted-ray returned by `refract(I, N, eta)` then points away from the surface in the correct (downward) half-space. The current code already passes `-V`, so the bug is in how the result is interpreted/used downstream OR the sign was inverted twice. Re-derive against `traceReflection` convention in `triangle.frag` and add a shader-side unit-style test.

Recommended: read https://registry.khronos.org/OpenGL-Refpages/gl4/html/refract.xhtml and walk a 45° incidence case before patching.

## Test

45° incidence with IOR=1.33 should produce a refracted-ray `tan(θ_t) ≈ 0.564` going DOWN; current code produces a ray going UP. Pixel-level golden-image diff at a known-good test camera angle.

## Completeness Checks

- [ ] **UNSAFE**: N/A — GLSL
- [ ] **SIBLING**: Cross-check `triangle.frag` `traceReflection` / IOR-refraction call sites use consistent convention
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Shader-side test asserting refracted ray direction half-space at 45° incidence

