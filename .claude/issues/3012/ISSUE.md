# SCR-D6-01: quest-event journal polled destructively before the frags.is_empty() early return

**Issue**: #3012
**Severity**: MEDIUM
**Dimension**: 6 — ECS scripting runtime
**Labels**: `medium,scripting,bug`
**Source report**: `docs/audits/AUDIT_SCRIPTING_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-16.md` (Dimension 6 — ECS scripting runtime).

**Location**: `crates/scripting/src/fragment.rs`:1534 (frags already in hand) and the journal poll preceding it

## Description

The quest-event journal is polled — **destructively claimed** — *before* the `frags.is_empty()` early return. When there are no fragments to run, the transitions have already been consumed and are then discarded.

## Evidence

`frags` is cloned at `fragment.rs`:1534 (`let frags = world.resource::<QuestStageFragments>().clone();`) after the journal has already been drained. Re-verified 2026-08-17.

Combined with #3010 — where `QuestStageFragments` is empty on every exterior launch — the destructive poll runs against an always-empty fragment table outdoors, so journal transitions are consumed and dropped on the floor.

## Impact

Quest stage transitions are silently lost whenever no fragment matches. Because the claim is destructive, there is no second chance: a later consumer cannot observe the event.

The two findings compound — #3010 guarantees `frags` is empty on exterior launches, which is exactly the condition under which this drops everything.

## Suggested Fix

Move the `frags.is_empty()` early return **before** the journal poll, or make the poll non-destructive (peek, then claim only what is actually dispatched).

## Related

- #3010 (SCR-D7-2026-08-16-01 — guarantees the empty-fragments condition on exterior launches)

## Completeness Checks
- [ ] **ORDER**: The early return precedes any destructive claim
- [ ] **SIBLING**: Any other destructive journal/queue drain in `crates/scripting` checked for the same ordering
- [ ] **LOCK_ORDER**: The existing TypeId-sorted resource acquisition (documented at :1530-1533) is preserved by the reorder
- [ ] **TESTS**: A regression test asserts transitions survive an empty-fragment tick

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3012 --json state` when live state is needed.*
