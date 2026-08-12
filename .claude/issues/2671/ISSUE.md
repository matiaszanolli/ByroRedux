# #2671: SCR-D6-NEW11-06: Alias match-CTDAs using RunOn::QuestAlias read the previous refresh's binding table, not the in-progress one

**Severity**: LOW
**Dimension**: Scripting Runtime Systems (Dimension 6)
**Untrusted-Input**: No
**Location**: `crates/scripting/src/scene.rs` -- `resolve_alias_bindings`'s condition evaluation, which resolves `RunOn::QuestAlias` against the committed `SceneActorBindings` rather than the in-progress `resolved` map
**Status**: NEW

## Description

During an alias-binding refresh, aliases are resolved in order into a local `resolved` map, which is committed to `SceneActorBindings` at the end. But an alias whose match conditions reference a *sibling* alias via `RunOn::QuestAlias` evaluates against the committed table, not the in-progress map.

So a sibling filled earlier in the **same** refresh is invisible: the condition sees last refresh's binding, or none at all on the first refresh.

## Evidence

The condition path resolves `RunOn::QuestAlias` through `SceneActorBindings::resolve`, which reads the committed table; the fill loop accumulates into a separate local `resolved` map that is not consulted by condition evaluation.

Self-corrects on the next refresh, once the sibling's binding has been committed.

## Impact

A one-refresh lag on cross-alias conditional fills. Bounded, because refreshes are frequent (driven by the dirty flag) and the state converges after one extra tick.

The visible symptom would be a conditionally-filled alias appearing one refresh late -- most likely invisible in practice, but it makes cross-alias conditions order-dependent in a way the design does not intend.

## Related

`docs/engine/m47-3-quest-alias-design.md` (alias fill ordering); SCR-D6-NEW11-04 (same fill loop)

## Suggested Fix

Evaluate `RunOn::QuestAlias` against the in-progress `resolved` map first, falling back to the committed `SceneActorBindings` table for aliases not yet visited in this pass.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives, other parsers, other spawn paths)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
