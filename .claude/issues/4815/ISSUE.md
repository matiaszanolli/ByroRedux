# #4815 — GAME-D7-2026-09-24-01: Loading a save discards the saved AI-procedure state whenever the pre-load session's clock selects a different package

**Labels**: medium,gameplay,ai,save-load,bug
**Filed from**: docs/audits/AUDIT_GAMEPLAY_2026-09-24.md

From `docs/audits/AUDIT_GAMEPLAY_2026-09-24.md` (HEAD `aabd99a05`).

- **Severity**: MEDIUM
- **Dimension**: 7 — Saved-or-rederived (also Dim 5)
- **Location**:
  - `byroredux/src/save_io.rs:171-175`: `PRE_RELOAD_RESOURCES` holds only the three reference ledgers.
  - `save_io.rs:1713` (reload inside `without_parked_state`), `:1750` (`restore_resources`), `:1779` (`apply_deltas`).
  - `byroredux/src/npc_spawn/ai_package.rs:571-575`: the spawn-time pick reads `GameTimeRes`.
  - `ai_package.rs:684-735`: the first tick re-selects, and calls `clear_ambient_behavior` on a change.
- **Status**: NEW
- **Description**:
  1. The reload spawns NPCs synchronously. `apply_ai_package_behavior` picks each NPC's winner against the **live** `GameTimeRes` and quest state, because neither is a pre-reload resource.
  2. `restore_resources` then installs the saved clock and quest state.
  3. `apply_deltas` overlays the saved `WanderState`, `TravelState`, `Traveled`, `GuardState`, `PatrolState` and `Escorted`.
  4. On the first Update, `ambient_ai_package_system` re-selects against the restored clock. `AmbientPackageRuntime` is unsaved and still holds the spawn-time winner, so `changed` is true whenever the two clocks pick different packages. `clear_ambient_behavior` then deletes the state that was just restored.
  - This breaks the rule in `PRE_RELOAD_RESOURCES`'s own doc: a resource belongs there if a spawn-time path consults it.
- **Evidence**:
  - A probe test in a HEAD copy (`audit_probe_saved_travel_progress_depends_on_live_clock`) replays the load order. It spawns at the live hour, restores the saved hour, overlays a saved `Traveled`, and ticks once. The packages are Sandbox 08:00+12 h and Travel 20:00+2 h.
  - Output: `same_clock_keeps_traveled=true other_clock_keeps_traveled=false`.
- **Impact**:
  - Travel packages that had finished travel again.
  - Patrols restart from waypoint 0.
  - Wander and guard progress resets.
  - The outcome of a load depends on the session state before the load.
- **Trigger**: any game with scheduled PACKs, for example an FNV saloon. Save at night, then load from a freshly booted engine at its default hour.
- **Related**: #2014 (the saved procedure columns), #3789, #4135, #4136.
- **Suggested Fix**: add `GameTimeRes` and the condition resources that package CTDAs read to `PRE_RELOAD_RESOURCES`, after confirming the unload path doesn't need their live values. Alternatively, re-seat each `AmbientPackageRuntime.active_package_form_id` from the restored clock before the first tick. Keep the probe as a regression test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (both NPC spawn paths, sibling interaction kinds, other parked `ReferenceState` facts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (no guard held across `world.get`/physics queries)
- [ ] **SAVE**: If a saved shape changes, `FORMAT_MAJOR` is bumped and no `serde(default)` is added (#4465)
- [ ] **TESTS**: A regression test pins this specific fix
