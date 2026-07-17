# MEM-D3-02: Stale MeshRegistry doc comment claims freed-slot reuse that doesn't exist

**Labels**: low, performance, memory, documentation

**Severity**: LOW
**Dimension**: GPU Memory Pressure
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-16.md`

## Location
`crates/renderer/src/mesh.rs:39-43` vs upload path `:295-314`, drop doc `:587-589`

## Description
The `MAX_MESH_SLOTS` doc claims "A correct streaming session re-uses freed slots via drop-and-push"; the actual upload path is grow-only (same shape as the texture-registry finding MEM-D3-01, but with a practically-unreachable 16M-slot ceiling). Doc-only; contradicts the correct statement at the `drop_mesh` doc site 550 lines away, which says: "Handles stay stable: the dropped slot holds `None` forever. Re-using a handle would re-enter the same `GpuInstance.mesh_id` for a different mesh and produce silent data corruption."

Verified current: both doc comments still exist verbatim and still contradict each other — `MAX_MESH_SLOTS` (line ~39-43) claims slot reuse, `drop_mesh`'s doc (line ~587-589) correctly states handles stay stable forever.

## Impact
Doc-only; contradicts the correct statement at the `drop_mesh` doc site 550 lines away. No runtime behavior is wrong — `drop_mesh`'s actual implementation is correct (grow-only, stable handles); only the `MAX_MESH_SLOTS` comment is stale.

## Suggested Fix
Fix the `MAX_MESH_SLOTS` doc comment to match `drop_mesh`'s correct statement: slots are never reused, the 16M ceiling is why that's safe in practice.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only fix)
