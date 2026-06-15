**Severity**: LOW (defensive only — zero runtime cost) · **Dimension**: Telemetry & Origin Cost · **Status**: PARTIALLY FIXED since prior F6/PERF3-01 — constant unified, formula still duplicated
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-06-14.md` (F9)

## Description
The `RENDER_ORIGIN_SNAP` *constant* is now a single source of truth (`crates/renderer/src/vulkan/scene_buffer/constants.rs:318`, imported by both sites). Only the snap *expression* `(pos / SNAP).floor() * SNAP` remains hand-written in two places — `byroredux/src/render/camera.rs:161` and `crates/renderer/src/vulkan/context/draw.rs:632-635` — guarded by prose only.

## Evidence
Verified live: `render/camera.rs:161` → `(cam_pos / RENDER_ORIGIN_SNAP).floor() * RENDER_ORIGIN_SNAP`; `draw.rs:632-635` → `(Vec3::from_array(camera_pos) / scene_buffer::RENDER_ORIGIN_SNAP).floor() * scene_buffer::RENDER_ORIGIN_SNAP`. Identical expressions, separately maintained across the crate boundary.

## Impact
The two formulas MUST produce bit-identical origins or the per-instance rebase (draw.rs) and the uploaded relative view_proj (camera.rs) disagree, shifting geometry by up to 4096 units. Identical and safe today; a future one-sided edit silently desyncs with no compile-time guard.

## Suggested Fix
Extract `pub fn snap_render_origin(camera_pos: Vec3) -> Vec3` next to the const and call it from both sites.

## Completeness Checks
- [ ] **SIBLING**: Confirm both call sites (camera.rs + draw.rs) — and any future third site — route through the single helper
- [ ] **TESTS**: A unit test pinning `snap_render_origin` so the two consumers can never diverge
