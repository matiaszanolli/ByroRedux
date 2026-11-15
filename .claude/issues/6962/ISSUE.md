# #6962: TD2-2026-11-15-09: The perk entry-point composition's scheduler `Access` is hand-copied at five registrations; the crime, ownership and gold subsystems already export a `fn(Access) -> Access`

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6962
- **Labels**: low,scripting,concurrency,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Dimension**: 2 · **Severity**: LOW · **Effort**: small · **Labels**: `tech-debt` · **Existing**: none (#6701 added `Access::with` for exactly this; the perk block was not migrated)
- **Age**: 49aa54e36 (2026-11-05, `npc_combat_ai.rs`); the block predates the split
- **Location**: `byroredux/src/boot/schedule/update/combat/combat_input.rs:17-29`, `update/combat/npc_combat_ai.rs:26-38`, `update/player_animation/combat_damage.rs:38-49`, `update/interaction_and_crime/interaction.rs:119-126`, `byroredux/src/boot/schedule/early.rs:142-148`
- **Description**: the first two are byte-identical for 13 lines (`PerkEntryTable`, `Perks`, `StoryManagerRng`, `WeaponSubject`, `PerkFragments`, `PendingPerkEntries`, `PapyrusPlayerEntity`, `FormIdComponent`, `FormIdPool`, `SceneAliasCandidate`), including the same two comment blocks ("PERK-SUB-002 - the outgoing-damage composition ...", "PERK-HOOK-008 ..."); `combat_damage.rs` carries the same set interleaved with its own entries, and `interaction.rs` / `early.rs` a subset. `scripting` already exports `crime_gold_access`, `crime_faction_access` and `ownership_access` (`crates/scripting/src/crime_gold.rs:36`, `crime_faction.rs:184`, `ownership.rs:85`) and the scan follows them (`acquisition_scan.rs`, #6701). A composer that gains a resource (the next `PERK-HOOK-*`) must be declared at all five or the lock-order and access guards go stale at the ones it forgets.
- **Suggested Fix**: `pub fn perk_entry_composition_access(access: Access) -> Access` in `crates/scripting/src/perk_entry_points/mod.rs`, next to the composer, covering the shared set; the five registrations call `.with(perk_entry_composition_access)` and keep only their own extras. The `acquisition_scan` already resolves `fn(Access) -> Access` helpers, so no guard change is needed beyond adding the file to the `#6701` list at `boot/schedule/mod.rs:756`.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope or scheduler `Access` changes, TypeId-sorted acquisition and the access declarations stay correct
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
