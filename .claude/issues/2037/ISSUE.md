# GPU-D5-01: Bloom upload_params rewrites construction-invariant UBOs every frame

**Labels**: low, performance, renderer, bug

**Severity**: LOW
**Dimension**: GPU Pipeline
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-16.md`

## Location
`crates/renderer/src/vulkan/bloom.rs:451-488`

## Description
All 9 down/upsample param UBOs are pure functions of `self.extent`, fixed at construction, yet rewritten every frame. ~144 bytes of redundant host memcpy/frame; no extra barrier.

Verified current: `upload_params` (`crates/renderer/src/vulkan/bloom.rs:451-488`) still recomputes and writes all `BLOOM_MIP_COUNT` down-param and `BLOOM_MIP_COUNT - 1` up-param UBOs unconditionally every call, purely from `self.extent`/`f.down_mips`/`f.up_mips` extents that don't change between resizes.

## Impact
~144 bytes of redundant host memcpy/frame; no extra barrier. Negligible cost, trivial fix.

## Suggested Fix
Write once at `BloomFrame::new` (and on resize, which already recreates the pipeline).

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (e.g. asserting `upload_params` isn't called on steady-state frames, or that UBO contents are unchanged without a resize)
