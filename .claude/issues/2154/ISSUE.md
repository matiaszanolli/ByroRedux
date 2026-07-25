# 2154: SAVE-D3-02: SaveCommand::execute holds SaveRegistry+SaveState guards across the entire ~30-storage snapshot walk

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2154
**Labels**: bug, ecs, low

---

## Severity
LOW

## Dimension
ECS Lock Ordering & Deadlock — `/audit-concurrency` 2026-07-25

## Location
`byroredux/src/save_io.rs:451-520`

## Description
`execute` acquires `ResourceRead<SaveRegistry>` and `ResourceWrite<SaveState>` and holds both across `validate_world`, `validate_form_ids`, and `save_world` — which between them acquire read locks on ~26 component storages and ~7 resources. This is the widest single-hold edge fan-out in the process, safe today only because `DebugDrainSystem` (the sole executor of console commands) is `add_exclusive` and listener threads never touch `World`. As with CHARAL-D3-01 (filed separately), that invariant is not restated at the call site.

## Evidence
`save_io.rs:452,455` — neither guard dropped before `:479`/`:504`. Neither `SaveState` nor `SaveRegistry` is itself a registered save column, so the always-on same-thread tracker won't fire spuriously.

## Impact
No live deadlock. Documentation/robustness only — moving command dispatch off the exclusive lane, or adding a parallel system touching `SaveState`, would create a wide cycle surface with no compile-time or test-time guard.

## Trigger Conditions
Requires a scheduler change; unreachable today.

## Related
#2126 (closed, same finding class), #2017 (ring cursor), #2019 (remap logging).

## Suggested Fix
Drop `state` before `save_world` (only `state.dir` and the already-computed slot are needed after validation, both cheaply copied), and add the #2126-style exclusive-scheduling note for the `registry` guard that genuinely must stay alive.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
