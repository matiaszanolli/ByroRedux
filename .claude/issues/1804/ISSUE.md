# D2-NEW-03: Two-sided glass split runs on additive particle batches — 2x draws + a fully-culled vertex pass with zero compositing benefit

**Issue**: #1804
**Labels**: low,renderer,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (D2-NEW-03)

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (D2-NEW-03)

## Location
`crates/renderer/src/vulkan/context/draw.rs:1086-1088,1159-1172`; `byroredux/src/render/particles.rs:96`

## Description
Particles emit `two_sided: true` + `alpha_blend: true` (`particles.rs:96`), so every particle batch hits `needs_split = is_blend && two_sided` and dispatches twice (FRONT-cull then BACK-cull), excluded from indirect grouping. The split stabilizes TAA depth-winner flips on volumetric glass — a rationale requiring depth writes + order-dependent compositing. Particles have `z_write: false` and (post-#1649) the dominant presets are additive (order-independent); billboards are camera-facing, so the FRONT-cull pass rasterizes ~nothing while still shading the whole instanced batch.

## Evidence
`particles.rs:96` `two_sided: true`; `draw.rs:1088` `needs_split = is_blend && two_sided`; `:1159-1172` the two-pass dispatch.

## Impact
2x draw calls and 2x vertex invocations for all live particles; batch counts are small post-#1649 so absolute cost is minor, but the first pass is provably dead work for the additive/no-depth-write case.

## Related
#1649, glass-split design (Tier C plan).

## Suggested Fix
Narrow the split predicate, e.g. `needs_split = is_blend && two_sided && z_write`, or exclude order-independent blends (`dst == ONE`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other hot-path loops / other dirty gates)
- [ ] **TESTS**: A regression test pins this specific fix

