# #5867: GAME-D2-2026-10-15-03: A conversation stays open when the partner dies, bleeds out or enters combat

- **Labels**: low,bug,gameplay,dialogue
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-15.md`

- **Location**: `byroredux/src/combat.rs:963-1052` (no dialogue teardown); `crates/debug-ui/src/lib.rs:335-337` (dialogue doesn't pause); `byroredux/src/barter.rs:239-266` (`begin_barter` has no `Dead`/`player_can_act` gate)
- **Status**: NEW. Related: #5678 (the voice side for a despawned speaker).
- **Description**: `reconcile_dead_actor` cuts the voice line but leaves `DialogueSurfaceState` and `NpcDialogueTopic` in place, and entering bleedout or combat tears nothing down. While the page stays open:
  - it lists the corpse's topics (each click is refused);
  - `IsInDialogueWithPlayer` returns true for the corpse.

  When the player closes the page, the dead speaker's OnEnd fragments run. One that calls `ShowBarterMenu` opens barter with a corpse.
- **Suggested Fix**: Call `end_open_conversation` from `reconcile_dead_actor` and on bleedout or combat entry of the open partner. Gate `begin_barter` on `npc_refuses_dialogue`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **LOCK_ORDER**: If a RwLock/query scope changes, TypeId-sorted acquisition and the system's declared `Access` row are preserved
- [ ] **TESTS**: A regression test pins this specific fix
