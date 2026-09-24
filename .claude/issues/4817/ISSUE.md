# #4817 — GAME-D4-2026-09-24-03: `CombatDisposition` is stamped when the actor job starts, but saved death and parked state arrive only at completion — hostility can arm corpses and body-less roots mid-spawn

**Labels**: medium,gameplay,combat,bug
**Filed from**: docs/audits/AUDIT_GAMEPLAY_2026-09-24.md

From `docs/audits/AUDIT_GAMEPLAY_2026-09-24.md` (HEAD `aabd99a05`).

- **Severity**: MEDIUM
- **Dimension**: 4 (and 5)
- **Location**:
  - `byroredux/src/npc_spawn/resumable.rs:2088-2092`: `spawn_placement_root` stamps `CombatDisposition`, `Transform` and `GlobalTransform` at prepare time.
  - `byroredux/src/cell_loader/references/mod.rs:716-735`: the NPC job returns `Pending` across frames.
  - `byroredux/src/cell_loader/references/synth_child.rs:80-97`: `reference_state::restore`, which inserts the saved `Dead`, runs only at `Complete`.
  - `byroredux/src/systems/faction_hostility.rs:242-260`: the perceiver snapshot is every `CombatDisposition` holder without `Dead` that has a `GlobalTransform`.
- **Status**: NEW (introduced by 9789d8153)
- **Description**: during an async exterior spawn, the root is a valid perceiver frames before its body and its parked `dead` flag land. In that window:
  - `faction_hostility` can start combat for an actor the eviction ledger records as dead;
  - `combat_ai` moves its root;
  - it can strike at once, because the hit cooldown starts at 0.

  Restore then inserts `Dead` and ragdolls it at the displaced position. The same window lets a live hostile root with no mesh chase and strike while invisible.
- **Impact**: a corpse from an earlier visit can hit the player or shift while its cell re-streams. Invisible attacks are possible.
- **Trigger**: FNV exterior. Kill a hostile NPC, walk until its cell evicts, then walk back into range while it re-streams.
- **Related**: #4414; GAME-D4-2026-09-24-05 (no test covers the window).
- **Suggested Fix**: stamp `CombatDisposition` at completion, after `restore`. Or have `faction_hostility` require a completion marker such as the root's `FormIdComponent`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (both NPC spawn paths, sibling interaction kinds, other parked `ReferenceState` facts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (no guard held across `world.get`/physics queries)
- [ ] **SAVE**: If a saved shape changes, `FORMAT_MAJOR` is bumped and no `serde(default)` is added (#4465)
- [ ] **TESTS**: A regression test pins this specific fix
