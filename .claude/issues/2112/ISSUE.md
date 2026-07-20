# #2112: D6-01: skin.coverage counters go stale on a bailed (early-return) frame

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2112
**Labels**: bug, renderer, low, performance

---

**Severity**: low
**Dimension**: Skinning & BLAS
**Location**: `crates/renderer/src/vulkan/context/draw.rs:2256` (reset site), guards at `:2228`
**Status**: NEW

## Description
`self.last_skin_coverage_frame` is reset to `SkinCoverageFrame::default()` **after** the early-return framebuffer-empty guard (`if self.framebuffers.is_empty() { return Ok(false); }`), unlike `self.skin_dispatch_ran`, which the `#1796`/D6-02 fix explicitly moved to reset *before* that same guard (see the `// #1796 / D6-02` comment directly above the `skin_dispatch_ran = false` line). A frame that bails through the empty-framebuffers path (or the `ERROR_OUT_OF_DATE_KHR` path) retains the previous frame's `SkinCoverageFrame` instead of reading zero.

This is the same class of bug #1796/D6-02 fixed for `skin_dispatch_ran` and the pose-hash freeze — but the fix wasn't applied to the sibling `last_skin_coverage_frame` counter reset.

## Evidence
```rust
// draw.rs:2226-2228
// #1796 / D6-02 — reset before either early-return guard below so
// a bailed frame reads `false`; see the field doc on `skin_dispatch_ran`.
self.skin_dispatch_ran = false;
...
if self.framebuffers.is_empty() {
    return Ok(false);
}
...
// draw.rs:2254-2256 (after the guard)
self.last_skin_coverage_frame = super::super::skin_compute::SkinCoverageFrame::default();
```

## Impact
Cosmetic — surfaces only during rare resize/swapchain-recreate transients. `skin.coverage` console/telemetry output can show a stale prior frame's counts on a bailed frame instead of zero.

## Suggested Fix
Move the `last_skin_coverage_frame` reset (and the per-frame draw-call-count resets adjacent to it) above the `framebuffers.is_empty()` guard, mirroring the `skin_dispatch_ran` treatment from #1796/D6-02.

## Related
#1796 (D6-02) — fixed the identical class of bug for `skin_dispatch_ran` and pose-hash; this is the sibling counter it didn't cover.

## Completeness Checks
- [ ] **SIBLING**: Check for other per-frame reset fields between the guard and the #1796 fix site that have the same ordering gap
- [ ] **TESTS**: A regression test pins this specific fix

