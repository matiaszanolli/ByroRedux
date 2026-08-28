# Issue #3470

**Title**: ECS-2026-08-27-01: `#3034`'s full-period text-key arm also matches a zero-advance step, so the `dt == 0` startup priming tick fires every text key of every looping clip

**Labels**: medium, ecs, animation, bug

**Filed**: 2026-08-27 via `/audit-publish docs/audits/AUDIT_ECS_2026-08-27.md`

---

**Source**: `docs/audits/AUDIT_ECS_2026-08-27.md` — finding `ECS-2026-08-27-01` (MEDIUM, Dimension 10: Animation Runtime — text keys). Audited at `HEAD = 969d81c8`; re-verified against current code at publish time.

## Description

`visit_text_key_events` gained (under #3034) an arm for the case where a `CycleType::Loop` clip advances by an exact multiple of `duration` and lands back on the instant it started from. Its guard is:

```rust
} else if curr_time == prev_time && clip.duration > 0.0 && clip.cycle_type == CycleType::Loop {
```

But `prev == curr` on a `Loop` clip has **two** causes, and the `(prev, curr)` pair carries no period count to tell them apart:

1. N ≥ 1 full periods elapsed (the intended case), and
2. **zero** advance — `delta == 0.0`, which `advance_time`'s `Loop` arm turns into `local_time += 0.0; local_time %= duration`, leaving `prev_time == local_time`.

Case 2 is live. `App::resumed` runs `self.scheduler.run(&self.world, 0.0)` immediately after `self.setup_scene()` (`byroredux/src/app_events.rs:182`) to prime transform state, so every `AnimationPlayer` that exists after the cell/NIF load takes a `dt == 0.0` tick. Every looping clip on that tick reports `prev == curr` and fires **all** of its text keys at once, at engine start, before any of them was actually crossed. The same shape is reachable through `BYROREDUX_FIXED_DT=0`.

The fix's own sibling guard covers only `Clamp` (`text_key_settled_clamp_stays_silent_at_prev_eq_curr`, `crates/core/src/animation/mod.rs`); there is no test pinning a `Loop` clip at zero advance, and `text_key_full_period_advance_fires_every_key_once` asserts the *firing* behaviour for the identical `(0.5, 0.5)` pair.

## Evidence

```rust
// crates/core/src/animation/text_events.rs — visit_text_key_events
} else if curr_time == prev_time && clip.duration > 0.0 && clip.cycle_type == CycleType::Loop {
    …
    for (t, sym) in &clip.text_keys {
        visit(*t, *sym);
    }
```

```rust
// crates/core/src/animation/player.rs — advance_time
player.prev_time = player.local_time;
let delta = finite_time_delta(dt * player.speed * clip.frequency);
…
    CycleType::Loop => {
        player.local_time += delta;
        if clip.duration > 0.0 {
            player.local_time %= clip.duration;
```

```rust
// byroredux/src/app_events.rs:182
// M41.0 Phase 1b.x — Prime the scene's transform state BEFORE the event loop starts.
self.scheduler.run(&self.world, 0.0);
```

`advance_stack` (`crates/core/src/animation/stack.rs`) carries the byte-identical `Loop` arm and reaches the same visitor through `visit_stack_text_events`.

## Impact

Every text key of every looping clip in the scene is delivered as an `AnimationTextKeyEvents` component on the priming tick (`byroredux/src/systems/animation.rs`). The consumer is `cinematic_animation_event_system` (`byroredux/src/systems/cinematic.rs`), a `Stage::Update` exclusive that writes `ActorCinematicState`, `CinematicPresentationState` **and** `QuestStageState` — so a spurious batch can advance quest state and fire presentation events for keys that were never crossed. Blast radius is one tick per engine launch today.

There is a worse latent variant worth naming while fixing this: #3258's `finite_time_delta` (`crates/core/src/animation/player.rs`) folds a non-finite `dt * speed * frequency` to `0.0`, which is precisely case 2 — so a clip reaching the registry with a NaN/±inf `frequency` from **any** producer other than `byroredux::anim_convert` (whose `sanitized_clip_frequency` guards the NIF path) would fire every text key on **every frame, forever**. `advance_stack_survives_a_non_finite_clip_frequency` (`crates/core/src/animation/mod.rs`) builds exactly that clip; it has no `text_keys`, so the interaction is invisible to the suite.

## Suggested fix

Carry the advance explicitly instead of inferring it from the `(prev, curr)` pair. Cheapest form: have `advance_time` / `advance_stack` record the applied `delta` (or a `wrapped_periods: u32`) alongside `prev_time`, and gate the new arm on `delta.abs() > 0.0`. A narrower stopgap is to have the callers in `byroredux/src/systems/animation.rs` skip text-key emission when the frame's `dt` is non-positive, but that leaves the zero-`frequency` / zero-`speed` variants open. Add a `text_key_loop_at_zero_advance_stays_silent` test next to the existing `Clamp` sibling.

## Related

#3258 (`advance_time` NaN latch, CLOSED), #3034 (the arm this finding is about, CLOSED), #2082 (the `Reverse` sibling arm).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`advance_stack`'s byte-identical `Loop` arm and `visit_stack_text_events`; the `Clamp` / `Reverse` arms)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix (`text_key_loop_at_zero_advance_stays_silent`, plus a non-finite-`frequency` clip that *has* text keys)
