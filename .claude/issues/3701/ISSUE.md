# #3701 — ECS-2026-08-30-D10-01 (LATENT): AnimationLayer blend-in contributes exactly zero weight for the whole fade — the crossfade is a hard cut

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: MEDIUM · **Dimension**: Animation Runtime
**Location**: `crates/core/src/animation/stack.rs` (`AnimationLayer::with_blend_in` ~:80-99, `effective_weight` ~:87-99; `advance_stack` blend-timer block ~:212-219)
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-D10-01)

> **LATENT — no live repro.** `AnimationStack` is never `world.register`-ed nor inserted by production code. `byroredux/src/boot.rs:1043` declares `.writes::<AnimationStack>()`, but the registration block (`byroredux/src/boot.rs:562-667`) registers only `AnimationPlayer` and `RootMotionDelta`. `World::query_mut` returns `None` for a storage that was never created, so `animation_system_inner`'s stack pass early-returns at `byroredux/src/systems/animation.rs:799-801` on every real frame. The layer/crossfade subsystem is fully implemented, save-serialised and debug-inspectable, but has no runtime producer. **Do not hunt for an in-game repro — there is none.** Severity is set for latent-not-live; the first consumer that registers `AnimationStack` inherits this defect.

## Description

`with_blend_in` parks `weight` at `0.0` ("Starts at zero, ramps up"), but nothing ever ramps it. `effective_weight()` computes the blend-in progress and *multiplies* it into `self.weight`, which is still `0.0`, so the product is `0.0` for the entire blend-in window. `advance_stack` is the only writer of `layer.weight` and it assigns only at *completion* (`weight = weight.max(1.0)`), never during the ramp.

The incoming layer is therefore invisible to `sample_blended_transform` (its `ew < 0.001` cull fires) for the whole fade, then snaps to full weight in one tick, while the outgoing layer renormalises to `w = 1.0` and holds full influence until culled.

## Evidence

```rust
// crates/core/src/animation/stack.rs
pub fn with_blend_in(mut self, blend_time: f32) -> Self {
    self.blend_in_remaining = blend_time;
    self.blend_in_total = blend_time;
    self.weight = 0.0; // Starts at zero, ramps up.
```

```rust
let mut w = self.weight;                       // 0.0 for the whole blend-in
if self.blend_in_total > 0.0 && self.blend_in_remaining > 0.0 {
    let progress = 1.0 - (self.blend_in_remaining / self.blend_in_total);
    w *= progress;                             // 0.0 * progress == 0.0
}
```

No test pins mid-blend weight — the only `with_blend_in` assertions check `blend_in_total`, never `effective_weight()` mid-fade.

## Impact

Every KFM-driven transition (`apply_pending_transition` -> `AnimationStack::play`) is a hard cut plus a one-tick pop instead of a blend. Latent today, but it is the entire point of the layer stack.

## Suggested Fix

Make the blend-in a weight *source* rather than a multiplier — keep the ramp target in a dedicated field (default `1.0`) and have `advance_stack` write `layer.weight = target * progress` each tick. Add a test asserting the two layers' effective weights sum to ~1 at the midpoint.

## Completeness Checks
- [ ] **SIBLING**: The blend-out half of `effective_weight` checked for the mirror defect
- [ ] **TESTS**: A regression test pins mid-blend `effective_weight()` (not just `blend_in_total`)
