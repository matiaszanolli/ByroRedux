# REN-D14-2026-08-07-04: Skipped caustic dispatch leaves a frozen pool that composite keeps adding

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2507
**Finding ID**: REN-D14-2026-08-07-04 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 14 — Caustics
**Location**: `crates/renderer/src/vulkan/context/post_passes.rs::record_caustic_splat_pass`
**Status**: NEW

## Description
Both skip paths — the `caustic_failed` permanent latch and `tlas_handle(frame) == None` — bypass the *entire* body, including the `cmd_clear_color_image`. The accumulator retains its last contents in `GENERAL`, and `composite.frag` samples `causticTex` unconditionally (no RT/validity gate) and adds `albedo * causticLum` to `combined` on every subsequent frame. Because the accumulator is screen-space, the frozen pool does not track camera motion — it paints a fixed pattern over the whole scene until a resize recreates the slots. The doc comment's "at worst one stale caustic frame hangs around until resize" understates this: it is re-composited every frame, not once.

## Evidence
Everything, including the clear, is nested inside the guard:
```rust
if !self.caustic_failed {
    if let Some(ref mut caustic) = self.caustic {
        let tlas_handle = self.accel_manager.as_ref().and_then(|a| a.tlas_handle(frame));
        if let Some(tlas) = tlas_handle {
            caustic.write_tlas(...);
            let caustic_result = caustic.dispatch(&self.device, cmd, frame, camera_static);
            ...
```
and `composite.frag` has no gate:
```glsl
uint causticRaw = texelFetch(causticTex, causticPixel, 0).r;
...
combined = direct + indirect * albedo + caustic;
```

## Impact
Requires the TLAS to go `Some` → `None` (an `ensure_tlas_state` build/allocation failure after a successful build) or a `dispatch` `Err` (a `write_mapped` failure on a persistently-mapped host-visible UBO) — both rare — so the probability is low, but the consequence is a permanently-visible screen-locked artifact rather than a graceful degradation to "no caustics".

## Related
#479 (the SVGF-shaped permanent-failure latch this mirrors).

## Suggested Fix
On either skip path, record a one-shot `cmd_clear_color_image` on `slots[frame]` (with the existing GENERAL→GENERAL pre/post barriers) so the feature fails to *black* rather than to *frozen*; a `caustic_cleared_on_skip: [bool; MAX_FRAMES_IN_FLIGHT]` latch keeps it to one clear per slot.

## Completeness Checks
- [ ] **TESTS**: A regression test forces the TLAS-absent skip path and confirms the accumulator clears to black rather than freezing
