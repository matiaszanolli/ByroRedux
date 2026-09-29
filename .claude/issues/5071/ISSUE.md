# #5071 — SCR-D2-2026-09-29-01: apply_effect's nested-lock inventory omits SpellList / SpellCatalog / ActorValues taken through magic::{add_spell,remove_spell}; the #3949 scan cannot see call-outs

**Labels**: low, bug, scripting, concurrency, test-gap

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-29.md` — finding `SCR-D2-2026-09-29-01`

**Severity**: LOW

**Dimension**: Fragment Dispatch & Locks

**Untrusted-Input**: No

**Location**:
- `crates/scripting/src/fragment/effects.rs:600-650`: the doc block.
- `crates/scripting/src/fragment/effects.rs:712-730`: the arm, applied inline under the quest guards, unlike the deferred `SetEnemy`.
- `crates/scripting/src/magic.rs:153-219`: `query_mut::<SpellList>`, then `try_resource::<SpellCatalog>`, then `query_mut::<ActorValues>`.
- `crates/scripting/src/fragment/tests.rs:3856-3911`: the scan covers `fragment::SOURCES` bodies only.

**Status in report**: NEW. This is the same class as #3949 (closed, LOW).

## Description

all three acquisitions run nested under `resource_2_mut::<QuestStageState, QuestObjectiveState>()`, and
none of the three types is named in the inventory. The SKILL states that both magic types belong there. The guard scan
matches acquisitions inside the fragment bodies only, so anything one module away (`crate::magic`) is invisible to it.

## Impact

no cycle today. Every quest-resource system is exclusive, and the ABBA lane's two cycles do not involve these
edges. The Dim-2 checklist's delegated inventory is incomplete, and its guard is structurally blind to call-outs.

## Suggested Fix

add a "via `crate::magic::{add_spell,remove_spell}`" bullet that names the three types. Then either
extend the scan to `magic.rs`, or defer the arm the way `SetEnemy` is deferred.

Validated at HEAD 9fcfdc3fc: the nested-lock doc block above `apply_effect` (`crates/scripting/src/fragment/effects.rs`) names none of `SpellList`, `SpellCatalog`, `ActorValues`; the AddSpell/RemoveSpell arm calls `crate::magic::add_spell` / `remove_spell` inline, and `crates/scripting/src/magic.rs` takes `query_mut::<SpellList>`, `try_resource::<SpellCatalog>`, `query_mut::<ActorValues>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (any other `crate::` call-out from a fragment arm)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: the #3949 scan covers `magic.rs` (or the arm is deferred)
