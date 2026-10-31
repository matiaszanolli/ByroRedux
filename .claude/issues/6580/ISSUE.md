# ECS-2026-10-31-02: `mine_system` is a `HitEvent` producer outside the ordering pin

Labels: low,ecs,test-gap,bug
Source: docs/audits/ (2026-10-31 report)

**Severity**: LOW (no live bug; a guard gap)
**Dimension**: ECS / System & Scheduler Wiring
**Source**: `docs/audits/AUDIT_ECS_2026-10-31.md` ECS-2026-10-31-02

## Location
`byroredux/src/systems/mines.rs:280-297` (the `HitEvent` insert in `detonate`); registration `byroredux/src/boot/schedule/update/quest_scene.rs:19`; the pin `hit_event_consumers_run_after_every_producer` at `byroredux/src/boot/schedule/mod.rs:116-150`. Introduced by `c36ab4125` (placed proximity mines).

## Description
`HitEvent` is a one-frame marker that `event_cleanup_system` drains. `mine_system` writes it for every actor in the blast. Today it sits correctly in the `quest_scene` segment, which runs before `script_runtime` (`hit_event_system`, `reference_script_hit_system`, `obscript_hit_system`) and `player_animation` (`combat_damage_system`). The pin's producer list (`pending_trap_hit_flush_system`, the two script-event systems, `projectile_flight_system`, `combat_input_system`, `make_npc_combat_ai_system`) does not name it, so moving the segment or the system would silently stop mine damage landing. The audit-ecs rule is "A new producer outside that list needs a name in the test."

## Impact
None today. A reorder of the Update segments would fail no test.

## Suggested Fix
Add `"crate::systems::mines::mine_system"` to the producer array. Design note for the same change: `detonate` uses `events.insert`, which overwrites a `HitEvent` already on that target in the same frame (a melee hit and a blast on one actor). That matches the other producers' one-slot model, so it is a note, not a defect.

## Related
#6540 (GAME-D4-2026-10-31-02, filed separately from the gameplay audit: the one-slot `HitEvent` overwrite when two mines trip together). This issue covers only the missing ordering-pin entry.

## Completeness Checks
- [ ] **SIBLING**: Any other `HitEvent` producer added since the pin was written is named in the test
- [ ] **TESTS**: `hit_event_consumers_run_after_every_producer` includes `mine_system`
