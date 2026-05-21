# #1204 — NIF-DIM4-04: SSE tangent synthesis fallback bails on empty inline arrays

**Source**: docs/audits/AUDIT_NIF_2026-05-19_DIM4.md (Dim 4, MEDIUM)
**Severity**: medium / Labels: bug, medium, nif-parser, import-pipeline
**State**: OPEN (filed 2026-05-19)
**Related**: #1104 (REN-D16-002 Path-2 bitangent handedness)

## Cause

`crates/nif/src/import/mesh/bs_tri_shape.rs:133-170` synthesis gate tests `shape.normals` / `shape.uvs` which are empty after `try_reconstruct_sse_geometry` (reconstructed data is in local `positions / sse_normals / sse_uvs`).

## Fix

Extract `synthesize_tangents_yup` helper in `mesh/tangent.rs` (already deferred by `bs_geometry.rs:113-118`). Route SSE-reconstructed path through it. Unlocks both SSE and Starfield BSGeometry synthesis.

## Game / Risk

Skyrim SE. LOW risk (monotonic on quality).

## Estimated impact

Skyrim SE NPCs / creatures lacking VF_TANGENTS. Reduces input pressure on the buggy #1104 Path-2.
