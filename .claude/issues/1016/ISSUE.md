# Issue #1016

**Title**: REN-D2-005: Global vertex SSBO pending_vertices has no upper-bound — unbounded growth on streaming sessions

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — REN-D2-005
**Severity**: MEDIUM (potential unbounded-growth on streaming session with no drop_mesh calls)
**File**: `crates/renderer/src/mesh.rs:213-261, 442-546`

## Premise verified (current `main`)

When a new scene mesh is uploaded, vertices/indices are appended to `pending_vertices`/`pending_indices` (line 229). `geometry_dirty = true` is set only when `pending_vertices.len() > ssbo_vertex_count`. `rebuild_geometry_ssbo` (line 513) compacts only when ANY mesh slot is `None` (i.e. some `drop_mesh` ran), then `take()`s the old buffer onto `deferred_destroy` with `DEFAULT_COUNTDOWN ≥ MAX_FRAMES_IN_FLIGHT` (UAF-safe).

## Issue

`pending_vertices` has **no upper bound check** before `extend_from_slice` (line 229). A streaming session that never calls `drop_mesh` (or where `drop_mesh` always finds rc>0) accumulates monotonically. Each rebuild allocates a DEVICE_LOCAL buffer of `sizeof(Vertex) * pending_vertices.len()` — at Vertex=100 B (post-M-NORMALS), 10M vertices = ~1 GB. Hard ceiling on BAR-routed device memory + large allocator-block churn (old buffer survives 2 frames in `deferred_destroy` while new one is live). Cell unload calls `drop_mesh` so this only fires on broken cell-unload paths, but a defence-in-depth cap matches the existing pattern at `scene_buffer.rs:1325`.

## Fix

Add a soft cap + WARN (e.g. 16M vertices ≈ 1.6 GB) on `pending_vertices.len()` in `upload_scene_mesh`'s growth path, and a hard panic above 32M. Same for `pending_indices`. Mirror `scene_buffer.rs:1325` material-cap pattern (`defense-in-depth against unbounded-growth bugs`).

## Test

Unit test uploading N synthetic meshes without `drop_mesh`; assert `pending_vertices.len() < SOFT_CAP` after each upload; WARN counter increments at SOFT_CAP; panic at HARD_CAP.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Apply same pattern to `pending_indices`; cross-check against scene_buffer.rs:1325 material cap
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Unbounded-growth regression test

