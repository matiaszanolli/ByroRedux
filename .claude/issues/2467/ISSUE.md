# REN-D9-NEW-01: Zero-weight fallback diverges between skin_vertices.comp (identity) and triangle.vert (inst.model)

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2467
**Finding ID**: REN-D9-NEW-01 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 9 — Skinning
**Location**: `crates/renderer/shaders/skin_vertices.comp:131-134` vs `crates/renderer/shaders/triangle.vert:146-151`
**Status**: NEW

## Description
Both shaders take a defensive branch when a vertex's four bone weights sum to ~0. The raster path substitutes the instance's world matrix (`xform = inst.model`, already render-origin-rebased on the CPU). The compute path substitutes `mat4(1.0)`. Because the skinned BLAS is instanced into the TLAS with an **IDENTITY** transform ("skinned draws get IDENTITY because their BLAS already holds absolute world-space vertices"), the identity fallback writes the raw bind-pose/NIF-local coordinate into what the TLAS reads as absolute world space. The in-shader comment claims the branch "mirrors triangle.vert:153" and that "the rigid `inst.model` path doesn't apply here" — the second half is an assertion, not a derivation, and it is wrong for a skinned actor standing anywhere other than the world origin.

## Evidence
```glsl
// skin_vertices.comp:131-134
float wsum = boneW.x + boneW.y + boneW.z + boneW.w;
mat4 xform;
if (wsum < 0.001) {
    xform = mat4(1.0);        // → raw local position, written into an ABSOLUTE-space BLAS
```
```glsl
// triangle.vert:146-151
if (wsum < 0.001) {
    xform = inst.model;       // → correct world placement for raster
```
Reachability: the classic `densify_sparse_weights` importer path cannot produce a zero quad (unweighted vertices fall back to bone 0 @ 1.0). The Skyrim SE / FO4 packed-half path (`crates/nif/src/import/mesh/skin.rs:112-125`) is documented as pass-through, no renormalisation, no zero fallback, so a decoded all-zero weight quad reaches the Vertex struct unmodified.

## Impact
A single zero-weight vertex inside a skinned mesh drags that entity's BLAS AABB from the actor's bounding box out to the world origin. In an exterior cell that is a 10^5-unit-wide box instanced into the TLAS with identity — every shadow / reflection / GI ray that enters that volume pays triangle-intersection cost on a degenerate sliver, and can register spurious hits (the "long thin ribbon" class of artifact already described for the unrelated IDENTITY-`bone_world` dropout at `byroredux/src/render/skinned.rs:31-38`). Raster shows nothing wrong, so the symptom presents as an unexplained RT perf cliff / shadow streak on specific SSE-family actors.

## Related
`byroredux/src/render/skinned.rs:31-38` (IDENTITY bone-world dropout — different cause, same visual signature); `#651` / SH-6 (the sibling bone-index clamp, correctly mirrored across the two shaders).

## Suggested Fix
Make the compute fallback match the raster one — bind the instance model matrix into the skin dispatch (push constant or an extra SSBO read) and use it for the `wsum < 0.001` branch; or, cheaper and equally correct, make the invariant real at import time by renormalising / zero-filling in the SSE packed-half path so `wsum == 0` is unreachable, and turn the shader branch into a `debug`-only assert. Do not "fix" it by removing the branch.

## Completeness Checks
- [ ] **TESTS**: A regression test constructs a zero-weight vertex through the SSE packed-half import path and confirms it no longer reaches the shader with wsum==0 (or confirms the compute-shader fallback matches raster)
- [ ] **SIBLING**: Confirm `byroredux/src/render/skinned.rs:31-38`'s IDENTITY dropout is tracked separately and not conflated with this fix
