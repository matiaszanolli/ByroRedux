# REN-D19-03: Near-field LAND terrain ships a zero tangent, so every TX01 terrain normal map shades through the screen-space-derivative fallback

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2474
**Finding ID**: REN-D19-03 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 19 — Tangent-Space
**Location**: `crates/renderer/src/vertex.rs:148` (`Vertex::new_terrain`), consumed at `byroredux/src/cell_loader/terrain.rs:457` (`spawn_terrain_mesh`)
**Status**: NEW

## Description
`Vertex::new_terrain` hard-codes `tangent: [0.0, 0.0, 0.0, 0.0]`, and `spawn_terrain_mesh` is the only builder for near-field LAND tiles. `triangle.frag` gates Path 1 on `dot(vertexTangent.xyz, vertexTangent.xyz) > 1e-4`, so every LAND fragment takes Path 2 (screen-space derivative TBN) — including the per-splat-layer `perturbNormal` calls that apply the TX01 tangent-space normal maps. This is exactly the path the checklist wants reserved for "synthetic geometry with no tangent". Terrain UVs are a regular axis-aligned grid, so an authored/synthesized tangent is trivially derivable and would be exact.

## Evidence
- `crates/renderer/src/vertex.rs:165` — `tangent: [0.0, 0.0, 0.0, 0.0]` inside `new_terrain`.
- `crates/renderer/shaders/triangle.frag:452-463` — `perturbNormal(..., fragTangent)` per splat layer; `fragTangent` is zero for these vertices, so Path 1 fails and Path 2 runs.
- Contrast the **distant** band: `byroredux/src/cell_loader/terrain_lod_btr.rs:168-172` explicitly carries `mesh.tangents` through with the anisotropic XZ correction, and `lod_support.rs:96-97` does the same for object LOD. Near-field is the only band without tangents.

## Impact
Path 2's `T` is constant per triangle, so terrain normal-map detail is shaded with a piecewise-constant tangent frame instead of a vertex-smooth one — faceting along the LAND grid on normal-mapped ground, most visible on high-frequency rock/gravel TX01 maps at grazing angles. It also produces a shading discontinuity across the near/distant terrain LOD boundary, because the BTR band *does* use Path 1. Blast radius: all exterior worldspaces; interiors unaffected.

## Related
#2371 (EX-10/11 near-terrain correctness + distant LOD bands) is the natural home; REN-D19-01 (#2245) fixed a Path-2 handedness bug that terrain is the largest remaining consumer of.

## Suggested Fix
Have `spawn_terrain_mesh` fill the tangent lane. Because LAND UVs are a uniform grid aligned to world XZ, `T` is the normalized world +X direction re-orthogonalized against the vertex normal with `w = 1.0` (same construction `cell_loader/water.rs:118` already uses); alternatively route the tile through `synthesize_tangents_yup` for a fully general answer.

## Completeness Checks
- [ ] **TESTS**: A regression test confirms near-field LAND tiles carry a non-zero tangent and take Path 1
- [ ] **SIBLING**: Compare against `terrain_lod_btr.rs`'s tangent construction for consistency
