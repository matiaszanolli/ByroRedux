# #7298: SCR-D2-04: The Message.Show() value path takes six resources inline under the quest pair; the deferral rule and the apply_effect lock doc no longer describe it

- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment/vm/interp.rs:1192-1224`, `crates/scripting/src/messages.rs:525-565` (`show_message_awaiting` / `queue_message` / `alias_name` 463-496), `crates/scripting/src/story_manager.rs:783-786`
- **Status**: NEW
- **Description**: The lowered/statement `Message.Show` defers onto `DeferredFragmentEffects::messages` and `show_message` runs after the quest guards drop. The value form calls `show_message_awaiting` straight from the interpreter, i.e. under `QuestStageState`/`QuestObjectiveState` WRITE: `MessageCatalog` (read ×2), `PapyrusMessageAnswers` (write), `Globals` (read, `{Global=}` tags), `SceneQuestAliasRegistry`, `LocationAliasBindings`, `SceneActorBindings` (reads, `{Alias=}` tags) and `PendingScriptMessages` (write). 10f7c16fc already hit one cycle from this (`QuestStageState → MessageCatalog → SceneQuestAliasRegistry → QuestStageState`) and fixed it only by making the catalog a leaf. The direct edge `QuestStageState(W) → SceneQuestAliasRegistry(R)` for an alias-tag message still opposes `story_manager_dispatch_system`, which holds the registry (`aliases`) across its `try_resource::<QuestStageState>()` read. No unit test shows a box with an alias tag under the quest pair, so `BYRO_LOCK_ORDER_CHECK` cannot see it.
- **Evidence**:
  ```rust
  // story_manager.rs:783-786
  let aliases = world.try_resource::<crate::scene::SceneQuestAliasRegistry>();
  let stages = world.try_resource::<QuestStageState>()...
  // interp.rs:1219  (under the pair)
  match crate::messages::show_message_awaiting(self.world, form, &numbers) {
  ```
- **Impact**: Not a runtime deadlock today — every holder of the pair and the story manager are `add_exclusive` — but it breaks the "touches a resource outside the quest pair defers" rule and rests the safety solely on exclusive scheduling; the apply_effect doc inventory does not name these types (the VM-native disclaimer covers it only in prose).
- **Related**: 10f7c16fc, #6667.
- **Suggested Fix**: Issue the ticket inline (one resource) and push a `PendingMessage::ShowAwaiting(form, args, ticket)` flushed with the other deferred messages after the guards drop; add an alias-tag Show test under `BYRO_LOCK_ORDER_CHECK`.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
