# CONC-2026-11-08-4.1: VatsState is written by an Early exclusive that runs after its Early parallel reader, with no pin test or note

_Immutable snapshot of the issue as filed (GitHub is authoritative for state). Source: docs/audits/AUDIT_CONCURRENCY_2026-11-08.md. Labels: low,concurrency,ecs,bug,test-gap_

Found by `/audit-concurrency` 2026-11-08 (`docs/audits/AUDIT_CONCURRENCY_2026-11-08.md`, HEAD 0419437ab).

- **Severity**: LOW (test-coverage / documentation gap; behaviour is benign)
- **Dimension**: Scheduler Proof Soundness (cross-stage sequencing)
- **Location**: writer `vats_mode_system` (exclusive) in `byroredux/src/boot/schedule/early.rs`; reader `player_controller_system` (parallel, Early; `byroredux/src/systems/character.rs`); reader in `byroredux/src/app_events.rs` (dt scale, before the tick)

## Description
The VATS mode machine is the single writer of `VatsState` and is deliberately an Early exclusive so it runs after this frame's `ActionState` edges. The exclusive phase follows the parallel batch, so `player_controller_system`'s VATS movement gate sees the previous frame's mode. The app loop reads `VatsState` before the scheduler runs to scale `dt`, so the world freeze also trails the mode edge by one frame (the enter frame runs at full `dt`, the frame after exit runs at `dt=0`). All other readers (`player_can_act` in `combat_input`, interaction, …) run in Update or later and are fresh. Unlike `WindField` (`player_wind_read_is_declared_and_weather_writer_is_exclusive`), nothing pins or documents this lag; the `early.rs` comment records only the ActionState-edge rationale.

## Impact
At most one frame of movement or world time at the enter/exit edge. No lock or data hazard.

## Suggested Fix
Add a one-line note beside the `vats_mode_system` registration and a small `scheduler_access_tests` pin asserting the writer is Early exclusive and the `player_can_act` consumers (`combat_input_system`, `interaction_system`) are Update or later. Do NOT move the writer — the behaviour looks intentional; make it deliberate.

Related: #6350 (VATS), #6625.

## Completeness Checks
- [ ] **SIBLING**: other Early-exclusive writers read by Early parallel systems are pinned or documented
- [ ] **TESTS**: a schedule pin reads the built schedule (`runs_before`), not registration text
