# #6236: ECS-2026-10-21-05: `TrapMotionSamples` keeps unpruned, per-entity-untimed positions, so re-contact velocities are spurious

**Labels**: medium,ecs,scripting,gameplay,bug

**Source**: docs/audits/AUDIT_ECS_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: 4 Resources & World-Level State (per-entity fact in a global `EntityId` map) · **Location**: `byroredux/src/systems/script_queries.rs:95-136`

- **Location**: `byroredux/src/systems/script_queries.rs:95-136` (`TrapMotionSamples`, `trap_contacts`); consumer `crates/scripting/src/obscript_more/extras.rs:300-333` (`trap_update`, ObScript `TrapUpdate` 0x110c)
- **Status**: NEW (added with the M47.3 `TrapUpdate` work, `8d32b3fe9`). It differs from PERF-D1-01, which is `trap_hits.rs`'s ancestor-chain rebuild.
- **Description**: `TrapMotionSamples` keeps `positions` and `velocities` as `HashMap<EntityId, Vec3>` plus a single shared `time`. Each call updates only the entities overlapping that trap *this* call, then sets `time = now`. An actor that left contact keeps its old position. When it touches any `TrapUpdate` trap again, its velocity is `(position_now − position_when_it_last_touched) / dt`. Here `dt` is the time since the *last call by any trap*, which is one frame while a trap's GameMode runs. The result is a velocity of thousands of units per second. Nothing removes entries, so the map also grows with every actor that ever touches a trap.
- **Evidence**: `samples.positions.get(&entity).map_or(Vec3::ZERO, |previous| (position - *previous) / dt)`, with `samples.time = now` written once per call (:120-129). `trap_update` gates both damage and the `PushActorAway` request on `contact.relative_speed < min_speed` (`extras.rs:308`).
- **Impact**: On Oblivion/FO3/FNV, a trap whose script uses `fTrapMinVelocity` so that it harms only while moving hits and knocks down an actor that walks into it at rest, on the first frame of any re-contact. The non-continuous case ("damage on first contact only") is exactly that frame. Memory growth is small, but it never shrinks and the soak gate does not see it.
- **Related**: GAME-D4-2026-10-21-01 (the knockdown that such a spurious push triggers can freeze the actor); the skill's Dim 4 "global map keyed by `EntityId`" rule. Gameplay semantics are `/audit-gameplay`; this report owns the shape.
- **Suggested Fix**: Store `(position, sampled_at)` per entity and treat a sample older than one frame as absent (velocity zero). Or move the sample onto a sparse component that despawn reclaims. At a minimum, `retain` the map to the entities involved in this frame's calls.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
