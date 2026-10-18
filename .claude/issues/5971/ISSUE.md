# SCR-D1-2026-10-17-03: A VMAD `None` object property resolves to form 0 and effects act on it instead of skipping

Issue: #5971
Labels: medium, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D1-2026-10-17-03: A VMAD `None` object property resolves to form 0 and effects act on it instead of skipping

- **Severity**: MEDIUM
- **Dimension**: Recognizer Chain
- **Location**: `crates/plugin/src/esm/records/script_instance.rs:137-142` (`object_form_id`), consumed at `crates/scripting/src/fragment/effects/resolve.rs:18-22,52-57`, e.g. `fragment/effects/inventory.rs:170-172`, `quest_stages.rs:349`, `quest_stage_gate.rs:96-102,227`
- **Status**: NEW
- **Untrusted-Input**: Yes (VMAD property table)
- **Description**: `ScriptInstance::object_form_id` returns `Some(form_id)` for any `PropertyValue::Object { form_id, alias: -1 }`, including the null object (`form_id == 0`, how an unset `Quest`/`Form`/`Actor Property` is stored). Every `QuestRef::Property` / `ObjectRef::Property` that the lowering emits therefore resolves to `Some(0)` for an unset property, and the appliers then act on form 0 (`resolve_quest` -> `QuestFormId(0)`; `resolve_property_form_id` -> 0). The lowering side never defaults a receiver to 0, but the shared resolver does at run time. Papyrus semantics for a call on/with `None` is a logged no-op.
- **Evidence**: `AddItem(Item, n)` with `Item` unset -> `add_items(world, container, 0, n)` pushes `ItemStack::new(0, n)` into the inventory and emits `OnItemAdded` for form 0 (`inventory.rs:170-172,269-294`, no zero filter); `SomeQuestProp.SetStage(n)` with the property unset -> `stages.set_stage(QuestFormId(0), n)` creates a phantom quest-0 entry and a `QuestStageAdvanced` for it (`quest_stages.rs:349-362`); the recognizers read `myQuest` the same way (`quest_stage_gate.rs:227`) so a trigger with `myQuest` unset arms against quest 0. The only form-0 handling in the effects code is for alias-bound properties (`evaluate.rs:46,627`).
- **Impact**: An authoring mistake (or a hostile VMAD) writes phantom inventory rows / quest-state entries that are saved with the world instead of being skipped. Low frequency in vanilla, but the failure is state corruption, not a skip.
- **Related**: #2186 (strict alias == -1 check was centralised in this exact accessor).
- **Suggested Fix**: Make `object_form_id` return `None` for `form_id == 0` (single shared choke point, keeps `alias_property` unaffected), with a test that an unset property declines the effect.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
