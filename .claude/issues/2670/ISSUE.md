# #2670: SCR-D6-NEW11-05: The SAVE-D6-01 rekey silently drops an inventory grant when the entity carries no SceneAliasCandidate

**Severity**: LOW
**Dimension**: Scripting Runtime Systems (Dimension 6)
**Untrusted-Input**: No
**Location**: `crates/scripting/src/scene.rs:587` (`reference_form_ids` map build) and `:616` (`reference_form_ids.get(&entity)` guard), added by `c4c30afd`
**Status**: NEW

## Description

The SAVE-D6-01 fix rekeyed `QuestAliasInjectionState.inventory_grants` from `(QuestFormId, i32, EntityId, u32, u32)` to `(QuestFormId, i32, u32 /*reference_form_id*/, u32, u32)` -- correctly, since `reference_form_id` is a stable authored ESM FormID that survives an in-session cell reload where raw `EntityId`s do not.

But the new lookup silently **drops** the grant when the resolved entity has no `SceneAliasCandidate` to supply a `reference_form_id`: `if let Some(&reference_form_id) = reference_form_ids.get(&entity)` with no `else`.

## Evidence

`crates/scripting/src/scene.rs:616` is a bare `if let Some(...)` inside the grant loop; there is no logging or fallback on the miss path.

Unreachable today: every alias-bindable entity is stamped with a `SceneAliasCandidate` at REFR spawn (`byroredux/src/cell_loader/references/mod.rs:1105-1112`), so the lookup always hits.

## Impact

No live impact -- the miss path cannot currently be reached.

It becomes reachable with the Phase 4+ **Created Object** alias fill, which by definition produces entities with no authored REFR and therefore no `reference_form_id`. At that point an alias-injected inventory grant would be silently skipped rather than granted, with no diagnostic to distinguish it from "already granted".

## Related

SAVE-D6-01 / `AUDIT_SAVE_2026-08-07.md` (the fix this is a residual of, verified correct this pass); `docs/engine/m47-3-quest-alias-design.md` Phase 4+ Created Object fill; SCR-D7-NEW11-03 (the other place a missing stamp has consequences)

## Suggested Fix

Log at `warn` on the lookup miss rather than dropping silently, and revisit the keying when Created-Object alias fill lands -- those entities will need a synthetic stable key, not an authored REFR FormID.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives, other parsers, other spawn paths)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
