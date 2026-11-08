# CONC-2026-11-08-4.2: vats_mode_system is a HitEvent producer outside the hit_event_consumers_run_after_every_producer pin

_Immutable snapshot of the issue as filed (GitHub is authoritative for state). Source: docs/audits/AUDIT_CONCURRENCY_2026-11-08.md. Labels: low,concurrency,ecs,bug,test-gap_

Found by `/audit-concurrency` 2026-11-08 (`docs/audits/AUDIT_CONCURRENCY_2026-11-08.md`, HEAD 0419437ab).

- **Severity**: LOW (latent; today's order is correct)
- **Dimension**: Scheduler Proof Soundness (registration-order pin completeness)
- **Location**: `byroredux/src/systems/vats.rs` (`vats_confirm` inserts `HitEvent`, reached from `vats_mode_system`); producer list in `hit_event_consumers_run_after_every_producer`, `byroredux/src/boot/schedule/mod.rs`

## Description
`vats_confirm` inserts a `HitEvent` per landed VATS target (#6422 folds shots per target) but `vats_mode_system` is not in the producer list of the pin test. `HitEvent` is a one-frame marker that `event_cleanup_system` drains in Late, so a producer registered after its consumers silently drops every hit. Safe today because the producer is in `Stage::Early` and all four consumers (`combat_damage_system`, `hit_event_system`, `reference_script_hit_system`, `obscript_hit_system`) are in Update; a same-frame overwrite is not possible (`combat_input_system` is gated off during VATS by `player_can_act`).

## Trigger Conditions
Someone moves `vats_mode_system` to Update, or later than a consumer (e.g. to remove the lag in the VatsState finding), and the pin does not notice.

## Suggested Fix
Add `"vats_mode_system"` to the producer array — one line, same shape as #6580 (`mine_system`). The test reads the built schedule and already handles exclusive and stage positions.

Related: #6580, #6540, #4324.

## Completeness Checks
- [ ] **SIBLING**: re-grep `HitEvent` insertions for any other producer missing from the list
- [ ] **TESTS**: the pin stays green with the new name (a renamed/env-gated-off name panics rather than passing)
