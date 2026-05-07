# #889 — SK-D1-NN-03: Inline BSTriShape bone weights not renormalized after half-float decode

**Audit**: `docs/audits/AUDIT_SKYRIM_2026-05-06_DIM1_4.md` (Dim 1)
**Severity**: LOW
**Labels**: low, nif-parser, import-pipeline, bug
**Created**: 2026-05-07

## Sites
- `crates/nif/src/blocks/tri_shape.rs:736` (inline path — `read_vertex_skin_data`)
- `crates/nif/src/import/mesh.rs:1437-1449` (SSE-buffer twin)
- `crates/renderer/shaders/triangle.vert:120-146` (consumer — does NOT divide by `wsum`)

## Summary
Inline BSTriShape and SSE-buffer skin paths decode 4 × IEEE-754 half-precision weights and pass them through unchanged. `densify_sparse_weights` (NiSkinData path, `mesh.rs:2043+`) DOES renormalize. Half-float quantization (~1 part in 1024 per component) lets a 4-influence vertex drift up to ~0.4% off unit sum. Asymmetric quality on the same mesh depending on which decode path the asset hits.

## Fix
Renormalize once at decode time in `read_vertex_skin_data` (or in a small helper called by both inline and SSE-buffer paths). Skip when `wsum` already within `1e-4` of `1.0`. Preserve the `wsum < 1e-6` rigid-fallback path.

## Completeness
- SIBLING: ensure inline + SSE-buffer paths share one helper.
- TESTS: 4-influence vertex with weights summing to 0.997 (typical half-float drift) → post-decode sum within `1e-4` of `1.0`. Negative test for `wsum < 1e-6` rigid fallback.
