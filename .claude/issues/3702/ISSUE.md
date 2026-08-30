# #3702 — ECS-2026-08-30-D10-02 (LATENT): a non-playing layer never ticks its blend-out, so cleanup_finished can never remove it and it holds full weight forever

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: MEDIUM · **Dimension**: Animation Runtime
**Location**: `crates/core/src/animation/stack.rs` (`advance_stack` ~:167-226, the `if !layer.playing { continue; }` guard at ~:169-171; `cleanup_finished` ~:152-159)
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-D10-02)

> **LATENT — no live repro.** `AnimationStack` is never `world.register`-ed nor inserted by production code. `byroredux/src/boot.rs:1043` declares `.writes::<AnimationStack>()`, but the registration block (`byroredux/src/boot.rs:562-667`) registers only `AnimationPlayer` and `RootMotionDelta`. `World::query_mut` returns `None` for a storage that was never created, so `animation_system_inner`'s stack pass early-returns at `byroredux/src/systems/animation.rs:799-801` on every real frame. The layer/crossfade subsystem is fully implemented, save-serialised and debug-inspectable, but has no runtime producer. **Do not hunt for an in-game repro — there is none.** Severity is set for latent-not-live; the first consumer that registers `AnimationStack` inherits this defect.

## Description

`advance_stack`'s `if !layer.playing { continue; }` skips the *whole* per-layer body, including the blend timers — not just the clip clock. `cleanup_finished` only retains-out layers satisfying `blend_out_total > 0.0 && blend_out_remaining <= 0.0`, so a paused layer that `play()` scheduled for fade-out (`play` sets `blend_out_remaining` on *every* layer regardless of `playing`) keeps `blend_out_remaining == blend_time` forever, is never retained out, and — because `effective_weight()` evaluates its blend-out factor as `remaining/total == 1.0` — contributes its **full** weight to every subsequent blend indefinitely.

## Evidence

```rust
// crates/core/src/animation/stack.rs
for layer in &mut stack.layers {
    if !layer.playing {
        continue;          // skips the blend-timer decrements at the tail of the loop too
    }
```

```rust
pub fn cleanup_finished(&mut self) {
    self.layers.retain(|layer| {
        if layer.blend_out_total > 0.0 && layer.blend_out_remaining <= 0.0 {
            return false;  // unreachable for a layer whose timer never ticks
```

## Impact

A paused layer permanently pins the blend result and, at max channel priority, permanently overrides the live clip; each pause/`play` pair adds one immortal layer. Latent today — nothing in-tree sets `AnimationLayer::playing = false`.

## Suggested Fix

Move the blend-timer advance above the `!layer.playing` guard so fades are wall-clock and independent of clip playback, or have `cleanup_finished` also drop layers with `effective_weight() < 0.001 && !playing`.

## Completeness Checks
- [ ] **SIBLING**: `AnimationPlayer`'s equivalent paused path checked for the same timer-starvation shape
- [ ] **TESTS**: A regression test pauses a fading-out layer and asserts it is retired
