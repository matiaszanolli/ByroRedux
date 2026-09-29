# #4994: CONC-D4-2026-09-28-01: `PARALLEL_SYSTEMS` leaves out the cross-file hops that carry 17 acquisitions, and the scan skips `world.get`; 7 physics_sync types have no mechanical pin

- **Repo**: matiaszanolli/ByroRedux
- **Labels**: low,concurrency,ecs,test-gap,bug
- **Filed from**: docs/audits/AUDIT_CONCURRENCY_2026-09-28.md
- **Filed**: 2026-09-28

- **Severity**: LOW (test gap; no live hole today)
- **Dimension**: Scheduler Access Declarations
- **Location**: `byroredux/src/boot/schedule/mod.rs:486-545` (table + its doc), `:611-658` (`acquired_in`); hops at `crates/physics/src/sync.rs:152` (`crate::water::apply_buoyancy`), `byroredux/src/systems/character.rs:607,1087,1472`, `byroredux/src/systems/camera.rs:121`, `byroredux/src/systems/animation.rs:65`
- **Status**: NEW. Related: #4573 (closed; its fix added mode, whole names and comment stripping, but not the `get` forms or the cross-file hops), #4064 (closed; about which systems are covered), #3964 / #1787 / #2676 (the same class of gap shipping on this system).
- **Description**: The table's doc (`mod.rs:497-500`) says "a hop into a different file is listed here explicitly rather than followed". Three parallel systems have unlisted hops.
  - `physics_sync_system` lists only `sync.rs`. Its buoyancy phase lives in `water.rs`, called as `crate::water::apply_buoyancy(world, …)`, and `fn_body(sync.rs, "apply_buoyancy")` returns None, so the scan never goes there.
  - `acquired_in` recognizes only `query` / `query_mut` / `resource` / `resource_mut` (plus `try_`). It does not recognize `world.get::<T>` / `get_mut` / `has`.
- **Evidence**: I re-ran the scanner line for line in Python, then again with the hops added and the `get` forms recognized. Results (actual vs. what the guard sees):

  | System | Actual types | Guard sees | Invisible to the guard |
  |---|---|---|---|
  | physics_sync_system | 24 | 13 | PhysicsWaterConstants, Ragdoll, TotalTime, WindField, WaterPlane, WaterVolume, WaterSurfaceMesh, WaterFlow, WaterCurrentVolume, **WaterContact=write**, **WaterContactScratch=write** |
  | player_controller_system | 28 | 24 (PhysicsWorld only as read) | **PhysicsWorld=write** (`set_kinematic_translation`, `set_linear_velocity`), **PendingDeathReconciliations=write** (`combat.rs:105`), WindField (`water.rs:394`), ActorControlState and ActorVitals (`world.get`) |
  | make_animation_system | 17 | 16 | Children (`anim_convert.rs:23`) |

  Only 4 of the 11 physics_sync types are backstopped, by the hard-coded needle list in `scheduler_access_tests.rs:206-219`. The baseline Dim 4 cited that list as the guard for this hop. The other 7 have no pin, and 2 of them are writes.
- **Trigger Conditions**: A future edit to `water.rs`, `combat.rs::queue_dead_actor_reconciliation`, a physics `set_*` helper, or a `world.get` read inside a parallel body adds or changes an acquisition without updating the declaration.
- **Impact**: None live. All 17 are declared by hand today (0 undeclared across all 9 systems). The next slip ships green, and `known_conflict_count()==0` is computed from an incomplete row. On physics_sync, this exact slip has already shipped four times, each time through `water.rs` or diagnostic helpers.
- **Verification Path**: Add the hop tuples, then re-run `cargo test -p byroredux --bin byroredux -- system_access_declaration`. In my simulation, the extended table has 0 undeclared at HEAD, so the change lands green.
- **Related**: ECS-2026-09-21-D5-01 (its re-implementation already scanned `get`/`has`, but its suggested fix dropped them).
- **Suggested Fix**:
  - Add `(WATER_SRC, "apply_buoyancy")` to physics_sync.
  - Add `(COMBAT_SRC, "queue_dead_actor_reconciliation")`, `(PHYSICS_SYNC_SRC, "set_kinematic_translation")`, `(PHYSICS_SYNC_SRC, "set_linear_velocity")` and `(WATER_SRC, "weather_wave_adjustment")` to player_controller.
  - Add `(ANIM_CONVERT_SRC, "build_subtree_name_map")` to animation.
  - Teach `acquired_in` the `.get::<` / `.get_mut::<` (write) / `.has::<` forms.

Source: `docs/audits/AUDIT_CONCURRENCY_2026-09-28.md` (CONC-D4-2026-09-28-01) · HEAD `9e6f08870`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related systems / CI steps
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition and the `docs/engine/ecs.md` canonical order are preserved
- [ ] **TESTS**: A regression test pins this specific fix
