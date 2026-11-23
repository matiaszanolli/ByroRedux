# #7315 ECS-D1-03: Read cycle `RapierHandles → WaterContact → ActorVitals → RapierHandles`

Labels: high,ecs,concurrency,physics,water,bug

From `docs/audits/AUDIT_ECS_2026-11-22.md` (HEAD e2a4c54cc; re-verified at f5329b2f5).

- **Severity**: HIGH. An ECS lock-order cycle carries a HIGH floor. All three edges are read guards, but std `RwLock` prefers writers, so a read cycle can block behind a queued writer.
- **Dimension**: 1 Lock Ordering
- **Location**:
  - `crates/physics/src/water.rs` `apply_buoyancy_with_scratch`: holds the `RapierHandles` and `RigidBodyData` queries while taking `WaterContact`. The contact scan near line 704 does the same.
  - `byroredux/src/systems/water.rs` `water_damage_system`: holds `WaterContact` while taking `ActorVitals`.
  - `byroredux/src/systems/trap_hits.rs` `live_actor_bodies`: holds the `ActorVitals` query while calling `world.get::<RapierHandles>`.
- **Status**: NEW. `git blame` puts all three edges before the baseline, and they have never been reported.
- **Description**: the three systems together form a 3-cycle. The detector follows transitive reachability, so it counts. No test process runs all three with populated storages, so the bin lane stays green. A live cell with water, a trap and an actor in the water under the detector would trip it.
- **Impact**: a detector panic in a live trap or water cell. Release builds are not affected unless a writer interleaves.
- **Suggested Fix**: in `live_actor_bodies`, gather the live actors first and drop the `ActorVitals` guard before reading `RapierHandles`. That is the one-guard-at-a-time shape #5954 used. Add a fixture with all three storages under the detector. **Needs a detector run** to confirm the trip before the fix and that it is gone after.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other multi-storage gathers / other rows / other producers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, the process-wide order (and TypeId-sorted pair acquisition) is preserved; the `BYRO_LOCK_ORDER_CHECK=1` lanes (core, scripting, bin) stay green
- [ ] **TESTS**: A regression test pins this specific fix (for a lock-order fix: a fixture that holds every storage involved, so the detector lane can see the edge)
