# D6-01: First-sight bind_inverses are drained from the pool before draw_frame commits them — an early return permanently corrupts the entity's skinning palette

**Issue**: #1791
**Labels**: high,renderer,vulkan,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (D6-01)

**Severity**: HIGH
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (D6-01)

## Location
`byroredux/src/main.rs:1759` (drain) → `main.rs:1806` (draw_frame call) vs `crates/renderer/src/vulkan/context/draw.rs:2031,2118,2148,2243,2259` (early returns preceding the commit) vs `draw.rs:2643-2662` (actual upload)

## Description
`render_one_frame` calls `self.skin_slot_pool.drain_pending(...)` at `main.rs:1759`, which irrevocably removes entries from `SkinSlotPool::pending_uploads` (`crates/core/src/ecs/resources.rs:850-853`, `drain(..n).collect()`), *before* invoking `ctx.draw_frame(...)` at `main.rs:1806`. `draw_frame` has multiple early returns preceding the bind-inverse upload at `draw.rs:2643`: empty framebuffers (`Ok(false)` @2031), `ERROR_OUT_OF_DATE_KHR` on acquire (`Ok(true)` @2118 — fires on every resize / mode change), and fence/reset/begin error arms (@2148/2243/2259). On any of these, the drained first-sight `bind_inverses` are dropped; the caller's `Ok(true)/Ok(false)` arms (`main.rs:1828-1882`) perform no re-queue. `entity_to_slot` keeps the slot resident, so `allocate()` never re-queues the upload, and the persistent SSBO region for those slots is never written.

## Evidence
Call path `main.rs:1759 drain_pending` → `main.rs:1806 draw_frame` → `draw.rs:2118 return Ok(true)` (upload at 2643 unreached) → `pending_with_data` dropped, no re-queue. `skin_palette.comp` then computes `palette[slot] = bone_world[slot] × <uninitialized>` for the affected slots, consumed by both `triangle.vert` (set 1 binding 3) and `skin_vertices.comp`; the garbage skinned vertices feed the per-entity BLAS and TLAS.

## Impact
Skinned entity (NPC body part) renders as garbage geometry in both raster and RT and pollutes the TLAS with degenerate triangles for the entity's remaining lifetime in the cell — recovery only via despawn + 3-frame pool sweep + respawn. Trigger requires a first-sight frame (NPC spawn / cell load) to coincide with a swapchain-out-of-date frame (resize, fullscreen toggle) — narrow but real, most likely during startup cell loads where window setup and streaming overlap.

## Related
#1192 (sibling loss vector, fixed), D6-02 (same root cause).

## Suggested Fix
Make the drain transactional — move `drain_pending` inside `draw_frame` past the last early return, or have `draw_frame` report "skin section reached" and re-queue the drained `(slot, entity)` pairs into `pending_uploads` on any path that returned before the upload.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

