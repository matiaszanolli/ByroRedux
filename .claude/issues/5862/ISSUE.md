# #5862: GAME-D4-2026-10-15-05: Script `DamageActorValue` / `SetActorValue` on Health has no death edge

- **Labels**: medium,bug,gameplay,combat,scripting
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-15.md`

- **Severity**: MEDIUM
- **Dimension**: Combat & Death (Dim 4), with Dim 3 (the actor-value writers)
- **Location**: `crates/scripting/src/fragment/effects.rs:515-549` (`DamageActorValue`, `02f22d793`); `:477-513` with `crates/scripting/src/fragment/effects/evaluate.rs:565` (`SetActorValue` / `SetActorValueNamed`)
- **Status**: NEW (the script-native sibling of #4702 and of the baseline's console finding, fixed by #5728)
- **Trigger**: a quest or reference script calling `DamageActorValue("Health", n)` / `DamageValue` / `SetActorValue("Health", 0)` on an actor or on the player.
- **Description**: These natives write the Health damage or base without `spare_from_death`, `Dead`, KILL or reconciliation. The SDK batch and console paths now call `commit_actor_value_deaths`; the script path does not. Nothing else sweeps actors at Health ≤ 0.
- **Impact**: A script-killed actor or player stays alive at ≤ 0 HP, and an essential actor never enters bleedout. Quests that wait on `OnDeath` stall.
- **Suggested Fix**: Route the script AV writes through the same `commit_actor_value_deaths` the console and SDK use, so that one helper is the only Health-to-death edge outside `combat_damage_system`. Test: a script `DamageActorValue` past zero marks the actor `Dead` and raises KILL; on an Essential actor it enters bleedout.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **LOCK_ORDER**: If a RwLock/query scope changes, TypeId-sorted acquisition and the system's declared `Access` row are preserved
- [ ] **TESTS**: A regression test pins this specific fix
