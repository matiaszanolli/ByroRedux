# SCR-D6-NEW3-03: Fragment-dispatch nested-lock safety depends entirely on undocumented scheduler wiring in a different crate

**Issue**: #2126
**Labels**: medium, sync, bug
**Dimension**: Scripting Runtime Systems
**Untrusted-Input**: No (structural/concurrency hygiene)
**Location**: `crates/scripting/src/fragment.rs:180-236` (nested lock acquisition in `apply_effect`), `byroredux/src/boot.rs:572-608` (the only place the safety invariant is stated, and it predates the newer lock surface)
**Status**: NEW (found in `docs/audits/AUDIT_SCRIPTING_2026-07-21.md`, Dimension 6)

## Description

`apply_effect`'s `AddItem`/`MoveTo` arms (added this session) acquire `Inventory`/`GlobalTransform`/`Transform` component locks while the caller (`quest_fragment_dispatch_system`) still holds `QuestStageFragments`/`QuestStageState`/`QuestObjectiveState` resource locks for the whole cascade loop.

Investigated in depth — **not a live deadlock today**: the scheduler (`crates/core/src/ecs/scheduler.rs:458-495`) runs parallel systems first, then exclusive systems strictly sequentially, and every system touching these quest resources (`quest_fragment_dispatch`, `quest_advance_dispatch`, the demo dispatchers) is registered `add_exclusive` in `byroredux/src/boot.rs` — so no concurrent holder can ever form the other half of an ABBA cycle.

But this safety property is enforced entirely by scheduler wiring in a different crate; `fragment.rs` itself has zero mention of "exclusive"/"parallel"/"Stage::" (confirmed by grep), and the `boot.rs` comment that does state the rationale predates (and doesn't account for) the newer component-lock nesting the `AddItem`/`MoveTo` effects introduced.

## Evidence

`crates/core/src/ecs/scheduler.rs:477-494` (parallel-then-sequential-exclusive per stage); `byroredux/src/boot.rs:580-582,608` (`add_exclusive` registration); zero hits for `grep -n "exclusive\|parallel\|scheduler\|Stage::" crates/scripting/src/fragment.rs`. No test or compile-time assertion pins `quest_fragment_dispatch_system` to the exclusive lane — every fragment test builds a bare `World` and calls the system function directly, never through the real scheduler, so an `add_to` vs. `add_exclusive` typo would pass every existing test.

## Impact

No live bug today. But the next contributor who parallelizes this system (a stated follow-up plan) or adds another object-targeting effect with its own component lock has no local signal in `fragment.rs` that doing so requires re-deriving this whole analysis. If it regresses, the failure mode is a genuine cross-thread ABBA deadlock (process hang) — HIGH once it happens.

## Suggested Fix

Add a doc comment directly on `apply_effect`/`quest_fragment_dispatch_system` stating the exclusive-scheduling dependency and listing every lock type it nests, not just the 3 resources. Optionally add a scheduler-level assertion/test that fails if this system is ever registered via `add_to`/`add_to_with_access` instead of `add_exclusive`.
