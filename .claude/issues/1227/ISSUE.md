# REN-D8-NEW-21: rt_flag warmup — first 1-2 frames render with RT disabled even after TLAS builds successfully

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1227
## Summary

The camera UBO's `rt_flag` is computed and uploaded near the top of `draw_frame`, but `tlas_written[frame]` only flips to `true` at the end of `build_tlas` → `write_tlas` much later in the same frame. So on the first frame in each FIF slot, `rt_flag = 0.0` is uploaded even when TLAS builds successfully — the shaders skip ray queries for that frame. Cosmetic warmup only.

## Evidence

- [draw.rs:409-414](crates/renderer/src/vulkan/context/draw.rs#L409-L414) — `rt_flag` computation, gated on `tlas_written[frame]`:
  ```rust
  let rt_flag =
      if self.device_caps.ray_query_supported && self.scene_buffers.tlas_written[frame] {
          1.0
      } else {
          0.0
      };
  ```
- The UBO upload runs as part of `upload_lights` BEFORE the render pass begins (line 405-408), and BEFORE TLAS build at `draw.rs:1158-1187`.
- `tlas_written: vec![false; MAX_FRAMES_IN_FLIGHT]` is the per-slot initialiser at `scene_buffer/buffers.rs:766`.
- With `MAX_FRAMES_IN_FLIGHT = 2`, both slots have to recur once for `rt_flag = 1.0` to land in the UBO — that's frames 0 + 1 with `rt_flag = 0.0`, then frame 2 onward correct.

## Impact

- Frame 0 and frame 1: flat shadows, no GI, no reflections, no caustics.
- Frame 2 onward: correct.
- TAA accumulates across the transition, so the visual is a brief flash that resolves within ~5 frames.
- This is the documented degraded mode for hardware without ray-query support, so the visual is "known". The annoyance is that it fires unnecessarily on RT-capable hardware on cold-start AND on every cell load that resets the TLAS.

**Not a correctness bug** — uploading `rt_flag = 1.0` before `write_tlas` would have the shaders try to ray-query a stale/null TLAS descriptor. The gate is intentional; the cost is the warmup.

## Suggested fix (low priority)

Two approaches:

1. **Hoist the camera UBO upload past TLAS build** (preferred). Split `upload_lights` so the camera-flags slot is written in a separate call after `write_tlas`. Adds one more UBO write per frame but eliminates the warmup.
2. **Patch `flags[0]` in-place after `write_tlas`**. Same effect, requires the camera UBO be `HOST_VISIBLE | HOST_COHERENT` (it likely is).

The mainline benefit only matters on cell-load boundaries — on a steady frame loop, `tlas_written[frame]` is sticky. So this is **enhancement, not blocker**.

## Completeness Checks
- [ ] **UNSAFE**: No unsafe involved.
- [ ] **SIBLING**: Verify no other UBO field has the same write-before-write order issue. `lights` itself is fine — it's written then read in the same frame.
- [ ] **DROP**: No Vulkan-object lifecycle change.
- [ ] **TESTS**: Hard to unit-test without a live Vulkan device. A bench-mode harness check ("first 5 frames after `--load-cell` should show RT contributions") would cover it indirectly.

## Source

[`AUDIT_RENDERER_2026-05-21_DIM8.md`](docs/audits/AUDIT_RENDERER_2026-05-21_DIM8.md) FINDING-D8-21.
