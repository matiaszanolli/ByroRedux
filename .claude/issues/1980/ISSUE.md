**Source:** FNV compatibility audit — Dimension 6 (Animation/Skinning/Particles), `docs/audits/AUDIT_FNV_2026-07-13.md`
**Severity:** LOW · **Status when filed:** NEW, CONFIRMED against current code

## Description
Both `advance_time` (`crates/core/src/animation/player.rs:76-90`) and `advance_stack` (`crates/core/src/animation/stack.rs:170-184`) reflect `CycleType::Reverse` time with a **single** fold: forward overshoot `local_time = 2*duration - local_time`, backward overshoot `local_time = -local_time`. If `delta = dt*speed*frequency` exceeds `2*duration` (a frame hitch on a very short reverse clip, or a large `speed`/`frequency`), one reflection is not enough and `local_time` lands back outside `[0, duration]`. There is no wrap loop or clamp on the reflected result — these match arms are the only writers of `local_time`.

## Evidence
- `player.rs:86`: `player.local_time = 2.0 * clip.duration - player.local_time;` with no subsequent range check; the backward branch `player.local_time = -player.local_time;` likewise.
- The `Loop` arm at the same sites uses `%=` + sign-fixup and is hitch-safe; `Reverse` is not.

## Impact
A transient out-of-range `local_time` for one frame. Self-limiting: the samplers (`find_key_pair` clamps before-first / after-last key) resolve it to an endpoint pose, so the visible effect is at most a one-frame pose snap that corrects next frame. Reverse-cycle clips are uncommon in FNV (idles are Loop, furniture is Clamp), so blast radius is small — hence LOW.

## Suggested Fix
Replace the single reflection with a triangle-wave fold over period `2*duration`, e.g.
```rust
let m = local_time.rem_euclid(2.0 * duration);
local_time = if m > duration { 2.0 * duration - m } else { m };
```
setting `reverse_direction` from the half, so any delta magnitude stays in range.

## Completeness Checks
- [ ] **SIBLING**: apply the same fold to BOTH `player.rs::advance_time` and `stack.rs::advance_stack` (both carry the single-reflection Reverse arm)
- [ ] **TESTS**: a regression test advances a short Reverse clip with `delta > 2*duration` and asserts `local_time ∈ [0, duration]`
