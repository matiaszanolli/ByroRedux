# #7301: SCR-D3-02: `RecurringUpdate` burst-fires after a long stall, against its documented "missed fires drop" contract

- **Severity**: LOW
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/recurring_update.rs:139-176`, test `recurring_update/tests.rs:71`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The module doc says missed fires drop and "burst-fire on resume … is uniformly worse". The re-arm is `seconds_until_next += interval_secs`. After a stall of N intervals the counter is still negative, so it fires again on every following frame until it is positive. A `dt` of 5.0 on a 0.5 s subscription fires on that tick, and then on about 9 consecutive subsequent ticks. The test `dt_overshoot_fires_only_once_per_tick` checks one tick only, so it cannot see this.
- **Evidence**: `recurring_update.rs:163-164`:
  ```rust
  // (the missed-fire contract — see module doc).
  ru.seconds_until_next += ru.interval_secs;
  ```
  The arithmetic is 0.5 - 5.0 = -4.5, then +0.5 = -4.0. The next tick at dt 0.016 gives -4.016 <= 0, which fires and re-arms to -3.5, and so on. `app_events.rs:1573-1608` passes the unclamped `wall_dt`.
- **Impact**: Demo-only today. Nothing in production inserts `RecurringUpdate` (only `dlc2_ttr4a.rs:210`; the #5842 comment in `byroredux/src/boot/schedule/update/player_animation/recurring_update.rs` says no recognizer emits it). It would be real the day a recognizer registers a handler.
- **Related**: none
- **Suggested Fix**: After a fire, clamp the counter to a single interval (`seconds_until_next = interval_secs - overshoot.rem_euclid(interval_secs)`, or reset). Extend the test to tick twice.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
