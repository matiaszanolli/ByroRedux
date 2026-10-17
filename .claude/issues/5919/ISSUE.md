# CHAR-2026-10-16-M-06: `ScriptedFactionChanges` is not in `PRE_RELOAD_RESOURCES`, so NPCs resident at a save-load lose their scripted faction membership

Labels: medium,character,bug,save-load,game:skyrim,game:fo4
Filed from: docs/audits/AUDIT_CHARACTER_2026-10-16.md

- **Severity**: MEDIUM
- **Dimension**: Population Boundary (writers vs stamps / restore order)
- **Game**: skyrim, fo4 (Papyrus-driven)
- **Location**: `byroredux/src/save_io/live_load.rs:32` (`PRE_RELOAD_RESOURCES`); consumer `byroredux/src/cell_loader/references/synth_child.rs:97` (`stamp_quest_reference` → `reapply_scripted_faction_changes`); `live_load.rs:659` (only the player is replayed after the reload)
- **Status**: NEW (introduced by `b3556ef6c`)
- **Source**: not numeric. The rule is `PRE_RELOAD_RESOURCES`'s own doc ("a resource belongs here only if some spawn-time code path consults it while the reload is actively building the new cell", #3789/#4136). `FactionRanks` is re-derived and not saved (`registry_completeness_tests.rs:600`, `REDERIVED_NOT_SAVED`), so the ledger is the only carrier.
- **Description**: The reload re-spawns the cell's NPCs, and each ends in `stamp_quest_reference`, which replays the *live* ledger. The saved ledger is installed only afterwards by the wholesale `restore_resources`; nothing re-applies it to NPCs already spawned.
- **Evidence**: `grep reapply_scripted_faction_changes` has exactly two production callers (`synth_child.rs:97`, `live_load.rs:659` for the player). `PRE_RELOAD_RESOURCES = ["ReferenceEnableState","ReferenceLockState","ReferenceScriptState"]` (re-read at HEAD).
- **Impact**: On "start engine, quickload" the live ledger is `Default`, so every NPC in the loaded cell returns with its record factions: a recruited follower's `PlayerFaction`, crime-faction changes and quest flags read by `GetInFaction`/`GetFactionRank` are silently reverted. In a same-session reload the post-save ledger replays instead of the saved one. Actors in other cells recover as they stream in.
- **Related**: #3789, #4136 (same class), #5058.
- **Suggested Fix**: Add `"ScriptedFactionChanges"` to `PRE_RELOAD_RESOURCES` (it has no teardown-side reader) and a live-reload test that places a ledgered NPC and asserts its rank after the reload.
- **Note**: Same class as closed #3789 / #4136.

---
Filed from `docs/audits/AUDIT_CHARACTER_2026-10-16.md` (HEAD fa7591ca2).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' rulesets, sibling writers/consumers)
- [ ] **TESTS**: A regression test pins this specific fix
