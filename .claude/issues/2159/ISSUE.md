# 2159: PERF-D9-NEW-01: camera_cut heuristic compares camera-relative matrices, misfires on ordinary motion and every origin crossing, defeating #1489

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2159
**Labels**: bug, high, performance

---

## Severity
HIGH

## Dimension
Telemetry & Camera-Relative Origin Cost (Dim 9) / temporal pipeline — `/audit-performance` 2026-07-25

## Location
`crates/renderer/src/vulkan/context/draw.rs:910-948` (also consumed at `:1491`)

## Description
Commit `6c56e311` (2026-07-19 — the same commit that caused `PERF-REGRESSION-6c56e311`) added an automatic camera-cut detector:
```rust
let vp_max_abs_delta = vp.iter().zip(self.prev_view_proj.iter())
    .map(|(a, b)| (a - b).abs()).fold(0.0_f32, f32::max);
let camera_cut = self.frame_counter > 0 && (camera_delta > 256.0 || vp_max_abs_delta > 0.75);
```
`vp` and `self.prev_view_proj` are **camera-relative** matrices (built by `assemble_camera` as `proj * look_at_rh(cam_pos - render_origin, ...)`, `byroredux/src/render/camera.rs:182-187`), and `self.prev_view_proj` is stored *un-corrected*, relative to the **previous** frame's origin. A raw element-wise comparison of two projection matrices is sensitive to (a) ordinary camera translation and (b) the 4096-unit render-origin snap — both make `camera_cut` true. When it fires, it calls `signal_temporal_discontinuity(8)` (SVGF recovery restart), `taa.signal_history_reset()` (zeroes `frames_since_creation`, forcing TAA back to first-frame mode), `fsr.signal_reset()`, and `previous_rigid_models.clear()`; it also sets `pvp = *vp` so the uploaded `prev_view_proj == view_proj` (zero camera motion vectors, bypassing `origin_corrected_prev_view_proj` entirely) and forces `previous_source = m` for every rigid instance (zero object motion vectors).

Confirmed against current code: `camera_cut` definition at `draw.rs:920-921` unchanged from the report's citation; consumed again at `:1491` gating the rigid motion-history map.

## Evidence
Reproducing the engine's default projection (`Camera::default().fov_y = FRAC_PI_4`, `perspective_rh` + Y-flip) at 16:9, the `0.75` threshold corresponds to:

| camera delta this frame | max\|ΔVP\| | camera_cut |
|---|---:|---|
| lateral 0.25 u | 0.34 | no |
| lateral 0.55 u | 0.75 | at threshold |
| forward 0.75 u | 0.75 | at threshold |
| forward 6.0 u (~360 u/s @ 60fps) | 6.00 | yes |
| render-origin snap, 4096 u (cell crossing) | 5562 | yes |

Bethesda-unit locomotion speeds (walk ~100 u/s, run ~350-400 u/s) give 1.7-6.7 units/frame at 60 fps — 2x to 12x over the trip point. `camera_delta > 256.0` (absolute positions) is the limb that *would* correctly catch a teleport; the VP limb is what misfires. The #1489/REN2-04 comment block and the exactness proof in `origin_corrected_prev_view_proj` (unit-tested) are both still correct — but on the one frame class they exist for (a grid crossing), the `camera_cut` branch takes precedence and the correction never runs. `grep camera_cut` finds no test coverage anywhere in the tree.

## Impact
Re-opens exactly the failure #1489 closed (full-screen TAA flash + SVGF history drop on every 4096-unit cell-boundary crossing) — and is far worse in the general case: while the player is moving at all, TAA's `frames_since_creation` is re-zeroed every frame, so the temporal resolve never accumulates; SVGF sits permanently at its recovery alpha (0.5) instead of steady-state (0.2); and FSR 3.1 — now the default upscaler — receives `reset=true` every frame, degrading a 66%-render-resolution reconstruction to a single-frame spatial upscale. Motion vectors are identically zero on those frames, so any denoiser/upscaler that survives the reset still reprojects incorrectly. This is a plausible additional contributor to `PERF-REGRESSION-6c56e311` (see the sibling issue filed for D5-01) that the `triangle.frag.spv`-swap bisect would not have isolated, since swapping only the SPIR-V leaves this host-side heuristic in place in both arms of that comparison.

## Related
#1489/REN2-04 (the fix this defeats), the D5-01 issue (same originating commit, possible compounding factor — filed separately), memory note "Renderer Ghosting Investigation Open", PERF-D9-NEW-02 (the diagnostic that should have caught this but reads stale state — filed separately).

## Suggested Fix
Compare *origin-consistent* matrices — run `origin_corrected_prev_view_proj` first, then diff `vp` against the corrected `pvp`, which removes the crossing false-positive outright. For the motion false-positive, drop the raw-element test in favour of an angular one (compare view-basis vectors or a reprojected far-plane corner set in NDC), or restrict an element test to the rotational 3x3 and give the translation limb a threshold in world units (the existing `camera_delta > 256.0` already covers that). Add a unit test pinning "1 cell-grid crossing + 6 units/frame of walking ⇒ no cut".

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (crossing + walking ⇒ no cut)
- [ ] **SIBLING**: FSR/TAA/SVGF reset signal paths all checked for the same false-positive dependency
