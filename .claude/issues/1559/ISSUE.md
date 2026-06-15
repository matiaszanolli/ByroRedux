**Severity**: LOW (no behavioral impact — flagged for consistency) · **Dimension**: BSTriShape Packed Geometry + SSE Reconstruction
**Location**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:926` (inline) vs `crates/nif/src/import/mesh/sse_recon.rs:216` (SSE-recon)
**Source**: AUDIT_SKYRIM_2026-06-14 (SK-D1-AUDIT-02)

## Description
The inline decoder gates the tangent quad on `VF_TANGENTS` alone (matching the nif.xml `BSVertexData` condition `0x11`); the SSE-recon decoder requires `VF_TANGENTS && VF_NORMALS`. No behavioral difference in practice — the byte stride is identical and the 5-tuple tangent assembly needs NORMALS anyway — but the two paths should use the same predicate so a future maintainer doesn't infer a real semantic divergence.

## Evidence
`bs_tri_shape.rs:926` = `let has_tangents = vertex_attrs & VF_TANGENTS != 0;`; `sse_recon.rs:216` = `let has_tangents = vertex_attrs & VF_TANGENTS != 0 && vertex_attrs & VF_NORMALS != 0;` (both confirmed live).

## Impact
None today; code-review consistency only.

## Suggested Fix
Align both to `VF_TANGENTS` (or both to `VF_TANGENTS && VF_NORMALS`) with a shared helper.

## Completeness Checks
- [ ] **SIBLING**: Any third decoder path uses the same shared predicate
- [ ] **TESTS**: Existing tangent stride tests still pass after alignment
