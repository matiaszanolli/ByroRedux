# SCR-D1-2026-10-17-11: The player's reference id (0x14) is special-cased in six places; `entity_global_form_id` documents itself as the inverse but is not for the player

Issue: #5979
Labels: low, scripting, bug, tech-debt

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D1-2026-10-17-11: The player's reference id (0x14) is special-cased in six places; `entity_global_form_id` documents itself as the inverse but is not for the player

- **Severity**: LOW
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/condition/eval.rs:106-125,142-148`; copies at `keywords.rs:91-97`, `fragment/distance.rs:166-170,179-185`, `fragment/reference.rs:593-599,821-827`, `fragment/effects/ai_combat.rs:112-117`, `fragment/effects/evaluate.rs:546-548`, `story_manager.rs:849-851`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `resolve_entity_by_global_form_id` handles `0x14` (#4694) but `entity_global_form_id` ("the inverse ... keyed the same way so the two agree") returns the player body's pool pair (local 1) instead, so each FormID-keyed ledger writer re-implements `if Some(entity) == player { 0x14 }` (five distinct copies, plus `distance.rs:168` re-doing the form->entity half the shared resolver already owns - exactly the drift the skill warns about). A new FormID-keyed effect that forgets the copy silently keys the player under the wrong id.
- **Evidence**: grep above.
- **Impact**: Maintenance / drift hazard only today.
- **Related**: #4694.
- **Suggested Fix**: Make `entity_global_form_id` (or one shared `reference_form_id(world, entity)` next to `resolve_entity_by_global_form_id`) return `0x14` for the player and delete the copies.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
