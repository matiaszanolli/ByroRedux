# PERF-D1-2026-09-21-05: The P2 combat systems allocate per frame while combat is active

**Labels**: bug, low, performance, gameplay, combat

Filed via /audit-publish from docs/audits/AUDIT_PERFORMANCE_2026-09-21.md.

**Severity**: LOW · **Dimension**: 1 — CPU Hot Paths
**Location**:
- `byroredux/src/systems/combat_anim.rs:333`: `let decisions = std::mem::take(&mut scratch.decisions);` in the P2 combat feedback system
- `byroredux/src/systems/combat_ai.rs:57-60`: `npc_combat_ai_system`'s fresh `decisions` / `steps` Vecs

**Status**: NEW (`ec3a18d2f`, 2026-09-21; `f61ea0447`, 2026-09-13)
**Verified against**: HEAD `73aaed7b9`; the code is identical to the audited `f97775ca8`.

## Description

- In `combat_anim.rs`, the sound-dispatch tail takes `scratch.decisions` with `std::mem::take` and never puts it back. The closure-persistent `FeedbackScratch` then regrows `decisions` from zero capacity (`scratch.decisions.clear()` at `:111`, then `push`) on every frame of an active hit or attack take. This is the `mem::take` capacity-churn pattern (cf. #3837, #3521).
- `npc_combat_ai_system` is a plain fn. It builds fresh `decisions` and `steps` Vecs every frame while any `AiCombatState` entity exists.

## Impact

A few small heap allocations per frame, only while combat is active. Allocation hygiene only.

## Related

- #4605 (CONC-D3-2026-09-21-02, open): the same two systems' guard lifetimes (hold-stack patterns). That is a separate defect, but fix both together.
- `docs/audits/AUDIT_GAMEPLAY_2026-09-21.md` cites this finding (per-frame combat allocations).
- #3837, #3521 (closed): earlier `mem::take` capacity-churn fixes, at other sites.

## Suggested Fix

Put the Vec back after the sound loop, or iterate `scratch.decisions` by index, so its capacity survives. Convert `npc_combat_ai_system` into a `make_*` factory with a persistent scratch, like `combat_anim`'s.

Source: docs/audits/AUDIT_PERFORMANCE_2026-09-21.md (PERF-D1-2026-09-21-05)

## Completeness Checks
- [ ] **LOCK_ORDER**: If the restructure changes when storage/resource guards are taken or dropped, the #4605 hold-stack fixes and TypeId-sorted acquisition are preserved
- [ ] **SIBLING**: The other combat / P2 systems added in the same wave are checked for fresh per-frame Vecs or unrestored `mem::take`
- [ ] **TESTS**: A regression test pins the fix (for example, `scratch.decisions` keeps its capacity across two frames with an active take)

