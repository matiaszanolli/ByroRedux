# SCR-D3-2026-10-17-03: Reference/alias-script `OnHit` is delivered before the projectile and `ProcessTrapHit` hits exist

Issue: #5991
Labels: medium, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D3-2026-10-17-03: Reference/alias-script `OnHit` is delivered before the projectile and `ProcessTrapHit` hits exist

- **Severity**: MEDIUM
- **Dimension**: Event Runtime
- **Location**: `byroredux/src/boot/schedule/update.rs:519-523` (`reference_script_event_system`) vs `:569,577` (`projectile_flight_system`,
  `pending_trap_hit_flush_system`); consumer `crates/scripting/src/fragment/reference.rs:605` (`collect_deliveries`) and `:858` (`relay_engine_events`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `HitEvent` is a one-frame marker drained in Late. #5872/ee6468fd1 moved the queued hits (projectile flight, `ProcessTrapHit`) to land
  at `update.rs:577`, "after the script systems ... before every consumer", and `hit_event_consumers_run_after_every_producer` pins
  `combat_damage_system` and `hit_event_system` after every producer. But `reference_script_event_system` is itself a `HitEvent` *consumer* (it turns
  the marker into the Papyrus `OnHit(akAggressor, ..)` delivery, and `relay_engine_events` into remote `OnHit` relays) and sits at `:521`, before
  both producers; the same guard lists it only as a *producer*. A projectile or trap hit lands at `:577`, is consumed by damage/RegisterForHitEvent,
  and is drained by `event_cleanup_system` before the next frame's `:521` ever runs, so `OnHit` never reaches a reference/alias script for those
  hits. Melee/NPC strikes (`:283/:346`, earlier) are delivered.
- **Evidence**: stage order above; `HitEvent` landed only via `trap_hit_requests.rs:62-78` for the queued sources; no other reader of the queue.
- **Impact**: scripts whose `OnHit` reacts to dart-trap / `Weapon.Fire` / `ProcessTrapHit` damage on their reference never run (the damage itself
  does land). One event class silently lost; no state corruption.
- **Related**: #5872, #5860, ee6468fd1.
- **Suggested Fix**: either register a second `HitEvent`-reading delivery after `pending_trap_hit_flush_system` (or move `reference_script_event_system`'s
  OnHit collection there), and add `reference_script_event_system` to the consumer list of `hit_event_consumers_run_after_every_producer`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
