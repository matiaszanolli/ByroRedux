# Issue #1024

**Title**: F-WAT-03: Regular TLAS-build path doesn't check is_water — future TLAS code would silently reintroduce self-hits

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — F-WAT-03
**Severity**: MEDIUM
**File**: `byroredux/src/render.rs` (TLAS-build path)

## Issue

`is_water` flag on `DrawCommand` does NOT exclude the entity from BLAS build. Today exterior water survives `evict_unused_blas` only because the water mesh deliberately skips BLAS at spawn — but the **regular** TLAS-build path does not check `is_water`. Any future code path that adds water meshes to the BLAS pool would silently reintroduce ray self-hits (water rays should reflect/refract against opaque geometry, not against the water surface itself).

## Fix

Make `is_water` a load-bearing predicate in the BLAS-build entry point: skip BLAS create OR mark water BLAS as a separate TLAS instance type that water rays exclude via instance mask.

## Completeness Checks
- [ ] **SIBLING**: Document the contract in mesh.rs / scene_buffer.rs near the water flag definition
- [ ] **TESTS**: Regression test asserting water entity's BLAS slot is None after upload

