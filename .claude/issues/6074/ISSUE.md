# GAME-SD-04: Restoring the snapshot's `active_package_form_id` makes the first ambient tick see "no change", so an alias-injected package's behavior is never installed after a respawn

Labels: medium,gameplay,ai,bug

**Severity**: MEDIUM · **Dimension**: NPC spawn -> AI package selection (Dim 5)
**Location**: `byroredux/src/cell_loader/stream_snapshot.rs:372-380` (the overwrite; now ~`:383-386`), `byroredux/src/npc_spawn/ai_package.rs:834-873` (spawn-time winner), `:1004-1021`, `:1071-1072` (`if !changed { ... continue; }`)

## Description
At respawn `apply_ai_package_behavior` selects over the NPC's own `PKID` stack only, because the alias overlays are stamped after spawn (its own comment says so, `ai_package.rs:861-864`); `behavior.insert_at_spawn` installs that winner W0 (or nothing, for an alias-only actor). `stamp_quest_reference` then runs `restore_actor_snapshot`, which sets `runtime.active_package_form_id = Some(P_alias)` unconditionally. On the next Update `quest_alias_refresh_system` (`update.rs:442`) stamps `QuestAliasInjectedOverlays` and queues an `EvaluatePackageRequest`; `ambient_ai_package_system` (`:463`) then selects `P_alias`, compares it to the restored id, finds `changed == false` and `continue`s without `clear_ambient_behavior` / `insert_at_runtime` (`:1063-1072`). `report_package_transition` also sees `previous == active`, so no PF_ fragment fires either.

Trigger: Skyrim (or any game with quest aliases injecting `ALPC` packages): an actor whose running package came from a quest alias overlay (a follower, an escort, one of the 44 forced-ref aliases #4704 mentions) leaves a tile/cell and returns.

## Evidence
`restore_actor_snapshot` sets only the id (plus `TravelState` / `Traveled` markers); nothing re-derives the behavior from it. The id is written by the same code path that, on a fresh spawn, deliberately starts at W0 so the first tick *does* see a change. The stream_snapshot tests never build an `AmbientPackageRuntime`, so the interaction is unpinned.

## Impact
The actor stands with W0's behavior (typically Sandbox, or idle for an alias-only actor) while `AmbientPackageRuntime` says its alias package is running, until the next package change (a schedule boundary or a re-evaluation that changes the winner). A follower or escort stops following after a stream-out/in.

## Related
#4704 (alias-only actors need the runtime), #3299 (the snapshot), GAME-SD-02 (the `ScriptPackage` case is the benign mirror: the id is restored but the slot is gone, so the tick reverts to the schedule), SAVE-SD-04.

## Suggested Fix
Do not restore `active_package_form_id` as the "already installed" id. Either restore it only when the spawn-time winner equals it, or leave the runtime's id at W0 so the first tick reselects and installs the right behavior (the snapshot's `TravelState` / `Traveled` markers are re-applied after `insert_at_runtime`, so Travel progress still resumes). Test: an actor with an alias overlay is captured, respawned, and one `ambient_ai_package_system` pass installs the overlay package's behavior.

Source: docs/audits/AUDIT_GAMEPLAY_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-placement components absent from the eviction row)
- [ ] **TESTS**: A regression test pins this specific fix
