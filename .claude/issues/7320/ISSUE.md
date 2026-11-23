# #7320 ECS-D5-03: The `HitEvent` pin names `vats_mode_system`, which raises no hit; the real VATS producer's order is unpinned

Labels: low,ecs,concurrency,test-gap,bug

From `docs/audits/AUDIT_ECS_2026-11-22.md` (HEAD e2a4c54cc; re-verified at f5329b2f5).

- **Severity**: LOW
- **Location**: `byroredux/src/boot/schedule/mod.rs` (the producer list in `hit_event_consumers_run_after_every_producer`); `byroredux/src/systems/vats.rs`; `byroredux/src/commands/vats.rs`.
- **Status**: NEW. Closed #6813 added the pin on a false premise.
- **Description**: outside tests, `vats_confirm` (which inserts `HitEvent` / `HitShots` / `HitParts`) is called only from the `vats.confirm` console command (`commands/vats.rs:238`). It is not called from `vats_mode_system`, which only switches modes and scans. Console commands run in `debug_drain_system`, registered in Late after `event_cleanup_system`. The hit survives that frame's cleanup and is consumed next frame. That works only because of registration order, and no test pins the drain after cleanup. The pin's comment, and the `/audit-ecs` skill text ("VATS shots via `vats_mode_system`"), are wrong.
- **Suggested Fix**: replace the entry with a pin (or a `run_after`) that orders `debug_drain_system` after `event_cleanup_system`, and correct the comment and the skill text.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other multi-storage gathers / other rows / other producers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, the process-wide order (and TypeId-sorted pair acquisition) is preserved; the `BYRO_LOCK_ORDER_CHECK=1` lanes (core, scripting, bin) stay green
- [ ] **TESTS**: A regression test pins this specific fix (for a lock-order fix: a fixture that holds every storage involved, so the detector lane can see the edge)
