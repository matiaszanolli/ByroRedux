# PHYS-D3-2026-08-20-09: pull_dynamic's lock-ordering comment describes drops that no longer happen there

**Issue**: #3130 — https://github.com/matiaszanolli/ByroRedux/issues/3130
**Finding**: `PHYS-D3-2026-08-20-09`
**Labels**: documentation, low, tech-debt
**Filed**: 2026-08-20 (comprehensive `/audit-suite` sweep, 25 reports)

---

**Audit**: `docs/audits/AUDIT_PHYSICS_2026-08-20.md` — Dimension 3 (ECS Sync)
**Severity**: LOW · **Status**: NEW — doc rot introduced by `6c8f1058` (`Separate physics storage and resource guards`, the #2404 fix)

## Location
`crates/physics/src/sync.rs:1075-1080` (`pull_dynamic`)

## Description
The comment reads:

> Drop the `RapierHandles`/`RigidBodyData` read guards before taking the `Transform` write lock below — `updates` already carries everything needed. `character_controller_system` acquires the reverse pair (`Transform` read held across `RapierHandles`), so overlapping the two orders would be an ABBA risk (#2135).

Both guards are now dropped ~85 lines earlier (`sync.rs:1002-1003`) as part of the #2404 restructure, and the statements the comment annotates are `if updates.is_empty()` / `query_mut::<Transform>()`.

The lock-ordering *rationale* it records (the ABBA edge against `character_controller_system`, #2135) is still true and still worth keeping — only its placement and the drops it claims are stale.

Verified at HEAD: the comment sits immediately above `if updates.is_empty() { return; }` at `sync.rs:1081-1083`, with no `drop(...)` between it and the `Transform` write acquisition.

## Impact
None at runtime. It is the load-bearing comment of the function's lock discipline, so a reader checking the #2135 invariant against it finds it describing code that is not there.

## Related
#2404, #2135.

## Suggested fix
Move the comment up to the actual `drop(handles_q); drop(body_q);` site and reword the first clause so it states the invariant ("the `Transform` write lock is never taken while a `RapierHandles`/`RigidBodyData` read guard is live") rather than describing adjacent statements.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved — the #2135 ABBA edge against `character_controller_system` must stay documented wherever the comment lands
