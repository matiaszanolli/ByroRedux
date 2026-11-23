# #7321 ECS-D5-04: Door motion and four new `ActivateEvent` consumers are ordered by adjacency only

Labels: low,ecs,test-gap,bug

From `docs/audits/AUDIT_ECS_2026-11-22.md` (HEAD e2a4c54cc; re-verified at f5329b2f5).

- **Severity**: LOW
- **Location**: `byroredux/src/boot/schedule/update/quest_scene/scene_actions.rs`; `byroredux/src/systems/door.rs`; the flush pin in `boot/schedule/mod.rs`.
- **Status**: NEW (M66)
- **Description**:
  - `door_motion_system` reads `TwoStateTransitionBatch`, which `two_state_activator_system` drains at its head. So it must be registered immediately before the activator, and only the module doc says so.
  - `activation_flush_is_scheduled_before_every_activate_event_consumer` does not name `door_motion_system`, `player_seat_system`, `bed_use_system` or `jail_bed_system`. All four are ordered correctly today.

  A reorder would silently stop doors from animating.
- **Suggested Fix**: add the pair to a `schedule_order` guard or a `run_before`, and add the four consumers to the flush pin.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other multi-storage gathers / other rows / other producers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, the process-wide order (and TypeId-sorted pair acquisition) is preserved; the `BYRO_LOCK_ORDER_CHECK=1` lanes (core, scripting, bin) stay green
- [ ] **TESTS**: A regression test pins this specific fix (for a lock-order fix: a fixture that holds every storage involved, so the detector lane can see the edge)
