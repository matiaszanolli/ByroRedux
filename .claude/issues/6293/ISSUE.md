# #6293: ECS-2026-10-21-07: `obscript_hit_system` is not in the `HitEvent` ordering pin

**Labels**: low,ecs,scripting,test-gap,bug

**Source**: docs/audits/AUDIT_ECS_2026-10-21.md · **Severity**: LOW · **Dimension**: 5 System & Scheduler Wiring · **Location**: `byroredux/src/boot/schedule/mod.rs:116-148`

- **Location**: `byroredux/src/boot/schedule/mod.rs:116-148` (`hit_event_consumers_run_after_every_producer`, 3 consumers listed); `byroredux/src/boot/schedule/update.rs:617` (registration)
- **Status**: NEW
- **Description**: M47.3 split `obscript_hit_system` out of `obscript_object_system` precisely so that it runs after the projectile and trap-hit producers (doc at `crates/scripting/src/obscript_objects.rs:433-436`, comment at `update.rs:613-616`). Today it does: :617 is after `pending_trap_hit_flush_system` (:604) and `projectile_flight_system` (:596). But the pin that turned the dart-trap bug (`ee6468fd1`) into a guard lists only `combat_damage_system`, `hit_event_system` and `reference_script_hit_system`. Moving the ObScript consumer back beside `obscript_object_system` (:525) would pass every test, and `OnHit`/`OnHitWith` for projectile and trap hits would silently stop firing.
- **Suggested Fix**: Add `"byroredux_scripting::obscript_objects::obscript_hit_system)"` to the pin's consumer list.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
