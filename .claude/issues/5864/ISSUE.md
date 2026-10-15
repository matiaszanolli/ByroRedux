# #5864: GAME-D5-2026-10-15-01: The spawn-time package winner is reported before alias overlays or the restored clock are applied, so PF_ fragments run Begin/Change for a package the actor never ran

- **Labels**: medium,bug,gameplay,ai,scripting
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-15.md`

- **Severity**: MEDIUM (the real impact depends on what the fragments do; no census was run)
- **Dimension**: NPC Spawn → AI Package Selection (Dim 5)
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:800-862` (report at `:847`); alias overlay merge `:980`, `:1042`; the save-load reseat at `byroredux/src/save_io/live_load.rs:610`
- **Status**: NEW (from `fb0fff11c`)
- **Trigger**: Skyrim/FO4, the only games with PACK VMAD:
  - an actor whose quest alias injects a package that wins (every spawn of that actor);
  - a save load whose restored hour or quest state picks a different winner than spawn did.
- **Description**: `apply_ai_package_behavior` selects over the base PKID list only, with the clock and quest state live at spawn, and immediately reports `None → A`. The first tick merges `QuestAliasInjectedOverlays`, which are stamped after spawn, and the save-load reseat uses the restored clock. Either can pick `B ≠ A` and report `A → B`. Next frame `package_fragment_system` runs Begin(A), Change(A) and Begin(B). The load drain discards pending story events but not `PendingPackageTransitions`.
- **Impact**: Package Begin/Change fragments run for a package the actor never ran. If a fragment calls `SetStage` or moves the actor, quest state goes wrong.
- **Suggested Fix**: Report the transition only from the first-tick confirmation, so spawn stamps the behaviour without reporting. Also clear `PendingPackageTransitions` in the load drain beside the story events.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **LOCK_ORDER**: If a RwLock/query scope changes, TypeId-sorted acquisition and the system's declared `Access` row are preserved
- [ ] **TESTS**: A regression test pins this specific fix
