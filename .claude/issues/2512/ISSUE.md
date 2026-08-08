# REN-D19-04: perturbNormal Path 1 multiplies by the raw interpolated vertexTangent.w instead of clamping it to +/-1, unlike the three sibling TBN sites

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2512
**Finding ID**: REN-D19-04 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 19 — Tangent-Space
**Location**: `crates/renderer/shaders/include/material_sampling.glsl:170` (`perturbNormal`); same pattern at `crates/renderer/shaders/include/lighting.glsl:128` and `crates/renderer/shaders/triangle.frag:2288`
**Status**: NEW

## Description
`.w` is exactly ±1 **per vertex** (guaranteed at import by `crates/nif/src/types.rs:154` `bitangent_sign` → `clamp_sign`, and by #2246 for the Starfield UDEC3 path). It is *not* ±1 **per fragment**: the varying is linearly interpolated, so any triangle whose three vertices disagree on handedness yields `w ∈ (-1, 1)`, hitting 0 at the mid-line. `perturbNormal` then builds `B = vertexTangent.w * cross(N, T)`, a shortened (or zero) bitangent, while `T` and `N` stay unit length — the TBN is no longer orthonormal and the V-axis component of the normal-map sample is attenuated toward zero. The POM sibling in the same file and the RT sibling both clamp first: `material_sampling.glsl:43` `tangentSign = vertexTangent.w < 0.0 ? -1.0 : 1.0;` and `include/ray_hit.glsl:191` the same.

## Evidence
```glsl
// material_sampling.glsl:169-171  (Path 1)
T = normalize(T - dot(T, N) * N);
vec3 B = vertexTangent.w * cross(N, T);   // raw interpolated w
mat3 TBN = mat3(T, B, N);
```
vs. the clamped form 127 lines above it in the same file (`:43`) and in `ray_hit.glsl:191`.

## Impact
Mixed-sign triangles are rare in authored Bethesda content (UV-seam vertices are duplicated, so a triangle normally spans one shell), but they are reachable through `synthesize_tangents` / `synthesize_tangents_yup`, where the sign is derived per vertex from *averaged* `tan_u`/`tan_v` accumulators — a vertex sitting on a UV fold can legitimately land on the opposite sign from its neighbours without the mesh duplicating it. Result is a band of washed-out normal-map relief (and, at `w ≈ 0`, a degenerate `mat3` column) along that seam. Cheap to make impossible; currently only 2 of 5 TBN reconstruction sites are hardened.

## Related
REN-D19-02 / #2246 (import-side ±1 clamp — this is the fragment-side residual it does not cover); REN-D19-01 / #2245.

## Suggested Fix
In `perturbNormal` (and for consistency `lighting.glsl:128`, `triangle.frag:2288`), replace the raw multiply with `float s = vertexTangent.w < 0.0 ? -1.0 : 1.0; vec3 B = s * cross(N, T);`, matching the POM and `ray_hit.glsl` sites, and note in the comment that the per-vertex ±1 guarantee does not survive interpolation.

## Completeness Checks
- [ ] **SIBLING**: All 5 TBN reconstruction sites (`perturbNormal`, `lighting.glsl:128`, `triangle.frag:2288`, plus the already-hardened POM and `ray_hit.glsl` sites) use the same clamped-sign construction
- [ ] **TESTS**: N/A shader-side; visual confirmation via a mixed-sign UV-fold test asset if available
