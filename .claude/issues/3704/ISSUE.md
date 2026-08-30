# #3704 — ECS-2026-08-30-D10-04: text keys are still dropped when one frame advances more than a full clip period but not an exact multiple

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: MEDIUM · **Dimension**: Animation Runtime
**Location**: `crates/core/src/animation/text_events.rs` (`collect_text_key_events`, the #3034 arm, ~:63-117)
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-D10-04)

**Status note**: this is a **coverage gap in #3034's fix**, not a regression of it. #3034 and #3470 are both CLOSED and both remain correct for the cases they cover.

## Description

#3034's "fire every key once" arm is gated on `curr_time == prev_time`, i.e. a delta that is an *exact* integer multiple of `duration`. For `|delta| > duration` with a non-zero residual the playhead still traverses one or more complete periods, yet control falls into the ordinary forward or wrap branch and only the residual window is scanned.

Worked example on a `Loop` clip, `duration = 0.5`, `prev = 0.1`, `delta = 1.3` -> `local_time = 1.4 % 0.5 = 0.4`; `curr (0.4) >= prev (0.1)` so only keys in `(0.1, 0.4]` fire — keys in `(0.4, 0.5]` and `(0.0, 0.1]`, each crossed twice during the traversal, fire zero times.

## Evidence

```rust
// crates/core/src/animation/text_events.rs — the #3034 arm requires exact equality
} else if curr_time == prev_time
    && applied_delta != 0.0
    && clip.duration > 0.0
    && clip.cycle_type == CycleType::Loop
```

`dt` is raw unclamped wall-clock (`byroredux/src/app_events.rs:513`, `:531`, `:541` — `wall_dt` stored into `DeltaTime` with no ceiling), so a streaming/cell-load hitch of several hundred ms against a sub-second looping clip is a realistic producer.

## Impact

`AnimationTextKeyEvents` feeds `cinematic_animation_event_system`, which writes `QuestStageState`. A frame hitch on a short looping clip silently swallows the notification, with no counter or log.

## Related

#3034 (CLOSED), #3470 (CLOSED — the zero-advance guard on the same arm).

## Suggested Fix

Widen the arm to `applied_delta.abs() >= clip.duration` on `Loop` clips (fire every key once, then still scan the residual window), keeping the `applied_delta != 0.0` guard #3470 added.

## Completeness Checks
- [ ] **SIBLING**: The `Reverse` / `Clamp` cycle types checked for the same multi-period traversal case
- [ ] **TESTS**: A regression test drives a `delta > duration` non-multiple hitch and asserts every key fires
