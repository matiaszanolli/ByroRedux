# FNV-2026-10-18-02: `AddScriptPackage` skips the Dialogue installer gate, so a declined Say-To / off-target package replaces an NPC's behaviour with nothing

- **Issue**: #6051 (https://github.com/matiaszanolli/ByroRedux/issues/6051)
- **Labels**: medium, ai, gameplay, scripting, bug, game:fnv, legacy-compat
- **Source**: docs/audits/AUDIT_FNV_2026-10-18.md (snapshot as filed; GitHub is authoritative for state)

Found by the FNV compatibility audit of 2026-10-18 (`docs/audits/AUDIT_FNV_2026-10-18.md`, HEAD `af5063597`).

- **Origin**: D5-01. **Dimension**: Ambient AI × M47.3 ObScript.
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:1017-1031` (`scripted.or_else(select_active_package …)` then `AmbientBehavior::from_package`) and `:1079-1083` (`clear_ambient_behavior`, install only `if let Some(behavior)`); compare `dialogue_installs_ambient` at `:80`.
- **Status**: NEW. Introduced by `af5db229d`. Same shape as the fixed #5499 / #5722. `gh` search "AddScriptPackage" is empty.
- **Description**: #5722 made selection skip any Dialogue package the installer declines. The new script-package slot is read before that scan, so a declined package becomes `active` unconditionally. `from_package` returns `None`, the actor's previous behaviour is cleared, and nothing is installed. The slot stays and `mark_running` starts its clock. Must-complete packages (`PKDT` flags `0x3004`) never lapse, and a Dialogue package never stamps `Traveled`.
- **Evidence**: Probe over `FalloutNV.esm`: 729 `AddScriptPackage` call sites with a resolvable PACK, 171 of them Dialogue packages over 104 distinct packages. **27 of the 104 are statically declined** (5 Say-To, 22 targeting an NPC rather than the player), at **30 call sites in 7 scripts**; 26 of the 27 are Must-complete. Example: `ArcadeHooverDamTriggerScript` gives follower Arcade Gannon the Say-To package `ArcadeHooverDamDialoguePackage` and never removes it. The existing tests (`a_script_package_preempts_the_scheduled_package` and siblings) use only Travel packages. Not reproduced live.
- **Impact**: An FNV NPC (a companion in the Arcade case) stands idle, without its schedule or follow behaviour, until a script calls `EvaluatePackage` / `RemoveScriptPackage`.
- **Suggested Fix**: In the scripted branch, apply `dialogue_installs_ambient` to Dialogue-procedure packages and drop the slot (or fall through to the schedule) when the installer declines. The proper fix is a Say-To runtime, which also covers the five Say-To arcade packages #5722 declines. Add a Dialogue-type test beside the Travel ones.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the other FO3 / Oblivion / Skyrim paths that share this code
- [ ] **TESTS**: A regression test (or a repaired gate) pins this specific fix
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved

