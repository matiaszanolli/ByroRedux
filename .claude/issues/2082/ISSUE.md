# FNV-D6-01: Text-key events fire the wrong set on CycleType::Reverse (ping-pong) backward legs

- **Severity**: LOW
- **Labels**: low, animation, bug
- **Location**: `crates/core/src/animation/text_events.rs:20-46`, reached via `crates/core/src/animation/stack.rs:222` (Reverse-cycle `fold_reverse_time` arm at ~171-180) and `byroredux/src/systems/animation.rs`

## Description
`visit_text_key_events` infers "loop wrap-around" purely from `curr_time < prev_time` (no `CycleType`/direction parameter exists in its signature at all). Ping-pong `CycleType::Reverse` clips also produce `prev_time > curr_time` on every backward leg (via `fold_reverse_time` decreasing `local_time`) with no actual wrap — both times stay in-range. The wrap-handling branch then fires the complement of the keys actually crossed on that backward leg.

## Evidence
`text_events.rs`'s `visit_text_key_events` branches solely on `if curr_time >= prev_time { normal-range firing } else { wrap-around firing across [prev_time,duration] ∪ [0,curr_time] }`. `stack.rs`'s `CycleType::Reverse` handling calls `fold_reverse_time`, which decreases `local_time` on the backward leg, tripping the wrap branch incorrectly.

## Impact
Anim-driven events (footsteps, hit frames, IK hints) misfire on the backward half of Reverse-cycle clips carrying text keys. LOW: Reverse + authored text keys is a rare combination in vanilla FNV content (idle/gameplay clips are overwhelmingly Loop/Clamp). No crash or visual corruption.

## Suggested Fix
Thread `reverse_direction`/cycle type into `visit_text_key_events`; on a backward leg fire the closed interval `(curr_time, prev_time]` instead of the wrap complement. Add a ping-pong regression test.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (Reverse-cycle clip with text keys, assert correct keys fire on backward leg)
