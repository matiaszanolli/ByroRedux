# #5391: GAME-D5-2026-10-08-03: Eat/Sleep with a non-NearReference PLDT walk to a hash-random point around wherever the actor currently stands — re-picked on every reinstall and load (the save allowlist's "idempotent" claim is false)

**Labels**: medium,gameplay,ai,save-load,bug,game:fnv
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5391

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-08.md` — `GAME-D5-2026-10-08-03` (HEAD `00f580e09`)

- **Severity**: MEDIUM
- **Dimension**: Dim 5 / Dim 7
- **Location**: `byroredux/src/systems/eat_sleep.rs:97-116`; `byroredux/src/systems/travel.rs:117-126`; `byroredux/src/npc_spawn/ai_package.rs:133-136`; `byroredux/src/save_io/registry_completeness_tests.rs:570`
- **Status**: NEW
- **Trigger**: FNV, an Eat package whose PLDT is In-Cell, Near Current Location or Near Editor Location (418 of 706 Eat NPC-default references), or the equivalent Sleep package (324 of 617, including 142 In-Cell). Especially after an earlier package (Travel/Sandbox) moved the actor away from home.
- **Description**: `from_package` passes a PLDT target only for `NearReference`. For every other location type, `resolve_destination` falls back to `pick_wander_target(home, radius, form_id, 0)`, with `home` set to the actor's current `GlobalTransform` at first sight.
  - **"Near editor location" eats near the current spot.** An actor that a Travel package took to a bar eats near the bar, not at home.
  - **"In cell X" is ignored.** The actor sleeps in the nearest sit marker around its current position.
  - **Random walk before seating.** The actor first walks a random offset (up to the radius, or 512 BU when unauthored) and then searches seats within the radius of that random point, rather than taking "any chair within the location radius" (GECK Eat/Sleep Package).

  `EatSleepState` is unsaved, and its allowlist row justifies that with "resolve_destination is idempotent". For this path it is not: after a load, combat or handover the new pick is centred on the actor's new position.
- **Evidence**: `let home = world.get::<GlobalTransform>(npc)…; resolve_destination(world, target_form_id, radius…, form_id, home)` with `target_form_id = None` for non-NearReference PLDTs.
- **Impact**: Diners and sleepers drift wherever their previous package left them, walk random offsets, and can end up seated outside the authored location. The destination after a save load differs from the one before it.
- **Related**: PHYS-D4-2026-10-08-01 (no waypoints, `blocked` dropped); PERF D1-03.
- **Suggested Fix**: For the location-type cases, use the location's centre (editor location = the spawn placement, current location = no walk, cell = skip when not resident) as the destination and the seat-search centre. Correct the allowlist row.

## Completeness Checks
- [ ] **SIBLING**: Sandbox/Wander/Travel non-NearReference PLDT location types checked for the same current-position fallback
- [ ] **TESTS**: A regression test pins this specific fix
