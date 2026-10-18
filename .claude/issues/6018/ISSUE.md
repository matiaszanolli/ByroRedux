# SCR-D7-2026-10-17-02: Papyrus ModEvent registrations are never pruned on despawn, unload or load - the 4 096 cap fills cumulatively and then silently refuses all new registrations

Issue: #6018
Labels: medium, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D7-2026-10-17-02: Papyrus ModEvent registrations are never pruned on despawn, unload or load - the 4 096 cap fills cumulatively and then silently refuses all new registrations

- **Severity**: MEDIUM
- **Dimension**: Provider/Extender Layer
- **Location**: `crates/scripting/src/papyrus_provider/execute.rs:405-431` (insert + cap), `ir.rs:183-190` (`PapyrusModEventRuntime`, transient); no pruning site exists (`grep PapyrusModEventRuntime` -> only `execute.rs`, `ir.rs`, `runtime.rs`, `late.rs`)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: Registrations are keyed `(EntityId, PrincipalId, EventId)`. Entity ids are never reclaimed (`world_tests.rs:137 despawn_does_not_reclaim_entity_ids`), so a despawned (cell-unloaded) registrant is never matched again, yet its row stays. The module doc says scripts "refresh registrations from OnInit/OnLoad after a world replacement", which re-registers under NEW entity ids and adds rows. Nothing removes the old ones (no despawn hook, no clear in `live_load`, no `UnregisterAll` on unload). `MAX_PAPYRUS_MOD_EVENT_REGISTRATIONS` = 4 096 is therefore consumed by dead entities over a session of cell streaming / F9 reloads.
- **Evidence**: `apply_mod_event_registrations`: `if contains_key || len < MAX { insert } else { warn "limit exceeded" }`. With ~40 scripted refs per cell and a handful of cell entries / save loads the cap is reached; afterwards every `RegisterForModEvent` is dropped with a warn and the script receives no events for the rest of the session. Compounded by SCR-D7-2026-10-17-01 (OnInit re-registers on every spawn).
- **Impact**: Silent loss of ModEvent delivery in long sessions; also unbounded-until-cap stale memory and a per-frame grouping pass (`execute.rs:50-58`) walking dead rows whenever an event is pending.
- **Related**: #5640 (per-frame grouping), #5535 era lock work (unrelated).
- **Suggested Fix**: Drop `registrations` rows whose entity no longer has a `PapyrusProviderProgram` (cheap retain when events are pending, or on cell unload), and clear the resource in the live-load path.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
