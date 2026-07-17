# PERF-D9-04: Render origin is snapped twice per frame from independently-passed camera_pos

**Labels**: low, performance, bug

**Severity**: LOW
**Dimension**: Telemetry & Origin Cost
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-16.md`

## Location
`byroredux/src/render/camera.rs:160`, `crates/renderer/src/vulkan/context/draw.rs:2583-2584`

## Description
Render origin is snapped twice per frame from independently-passed `camera_pos` — once in `byroredux/src/render/camera.rs` (feeding the relative view-proj matrix) and again independently in `crates/renderer/src/vulkan/context/draw.rs` (feeding the inverse-VP-reconstruction paths). No measurable CPU cost (a Vec3 floor/multiply, twice). The finding is fragility, not performance: the "both call sites must receive the same un-jittered `camera_pos`" invariant is enforced only by convention; a future refactor that jitters one path could desync the two origins by one cell-width at the boundary. Latent today, not active.

Verified current: both `byroredux/src/render/camera.rs:160` and `crates/renderer/src/vulkan/context/draw.rs:2583-2584` independently call `scene_buffer::snap_render_origin` on separately-passed `camera_pos`/`cam_pos` values.

## Impact
No measurable CPU cost. The risk is fragility: a future refactor that jitters one call site's input (e.g. for TAA) without the other could desync the two origins by one cell-width at the boundary. Latent today, not active.

## Suggested Fix
Compute `snap_render_origin` once per frame and thread the single result through both consumers, removing the "both call sites must receive the same un-jittered `camera_pos`" convention-only invariant.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (e.g. asserting both call sites use a single shared origin value)
