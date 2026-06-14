# F-11.D1: DoF look_at degenerates when focus_dist → 0 (dormant guard gap)

**Issue**: #1525
**Severity**: LOW (dormant)
**Dimension**: TAA (Depth of Field, M37.5)
**Labels**: low, renderer, bug
**Source**: docs/audits/AUDIT_RENDERER_2026-06-14_DIM11.md (F-11.D1)
**Filed**: 2026-06-14

> Snapshot as filed (TD10-001). GitHub is authoritative for current state:
> `gh issue view 1525 --json state`.

## Description

Under DoF (`dof.aperture > 0.0`) the per-frame jittered view is built as:

```rust
// crates/renderer/src/vulkan/context/draw.rs:603-612
let focal_pt = pos + dof.focus_dist * fwd;
let jittered_view = Mat4::look_at_rh(
    jittered_eye - render_origin,
    focal_pt - render_origin,
    up,
);
```

As `focus_dist → 0`, the eye→center vector collapses to `focus_dist·fwd − lens` — loses its
forward component and points along the perpendicular lens offset → degenerate/sideways view
basis; if the disk sample is also ~0 (eye ≈ center), `look_at_rh` normalizes a near-zero
direction → NaN, which TAA propagates through the history blend. No `focus_dist > ε` guard.

## Impact

None today — dormant. `DofView::default()` ships `aperture = 0.0`
(`crates/renderer/src/vulkan/context/mod.rs:754`); the DoF branch is skipped and no
console/runtime path sets `aperture`/`focus_dist`. Materializes once a DoF console command
is wired and a user sets `focus_dist = 0`.

## Suggested Fix

Clamp `dof.focus_dist.max(0.01)` at the `effective_vp` build site, or guard the DoF branch
on `dof.focus_dist > ε`. ~2 LOC — fold into the DoF console-command commit.

## Completeness Checks
- [ ] SIBLING: no other renderer `look_at_rh` / projection-build site takes an unbounded
  user-supplied focal distance (pinhole path `draw.rs:616-618` reuses precomputed `view_proj`, unaffected).
- [ ] TESTS: regression test that `effective_vp` is finite for `aperture > 0, focus_dist = 0`,
  and forward axis preserved at small `focus_dist`.
- [ ] DROP / LOCK_ORDER / FFI / UNSAFE / CANONICAL-BOUNDARY: N/A (pure math at camera-UBO assembly).
