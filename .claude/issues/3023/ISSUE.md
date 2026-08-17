# SAVE-D4-01: three more saved EntityId carriers invisible to every pre-write gate

**Issue**: #3023
**Severity**: MEDIUM
**Dimension**: 4 — pre-write validation gates
**Labels**: `medium,ecs,bug`
**Source report**: `docs/audits/AUDIT_SAVE_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SAVE_2026-08-16.md` (Dimension 4 — pre-write validation gates).

**Location**: `crates/core/src/ecs/components/follow.rs`:59-61 (`FollowState.target_entity`) · `escort.rs`:74-77 (`EscortState.target_entity`) · `sandbox.rs`:54-57 (`Seated.furniture`); registered at `byroredux/src/save_io.rs`:265-270 · `crates/save/src/validate.rs` and `byroredux/src/save_io.rs`:577-616 (no check touches any of them)

## Description

Three more saved `EntityId` carriers are invisible to every pre-write gate. #2535 covered only the two cinematic types; these three were registered for save afterwards and no validation followed them.

## Evidence

```
$ grep -c FollowState crates/save/src/validate.rs   → 0
$ grep -c EscortState crates/save/src/validate.rs   → 0
$ grep -c Seated      crates/save/src/validate.rs   → 0
```

All three are registered as save-participating at `save_io.rs`:265-270. Re-verified 2026-08-17.

## Impact

A session-local `EntityId` written to disk is meaningless on reload unless it is remapped or validated. These three carry exactly that: an actor's follow/escort target and a seated actor's furniture reference.

On load they point at whatever entity happens to occupy that id — a silent mis-reference rather than a failure, which is what the pre-write gate exists to prevent.

## Suggested Fix

Extend the pre-write validation to cover all three, matching the treatment #2535 gave the cinematic types. Better: make the gate enumerate save-registered components carrying `EntityId` rather than listing them by hand, so the next registration cannot silently escape it.

## Related

- #2535 (the two cinematic `EntityId` carriers — same class, prior instance)
- #3024 (SAVE-D4-2026-08-16-02 — a fourth carrier, same dimension)

## Completeness Checks
- [ ] **ENUMERATED**: The gate derives its list from the registry rather than a hand-maintained set
- [ ] **SIBLING**: All three covered, plus #3024's `EquippedWeapon.inventory_index`
- [ ] **PARITY-GUARD**: A newly registered `EntityId`-carrying component fails the guard until validated
- [ ] **TESTS**: A regression test writes a dangling target and asserts the gate rejects it

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3023 --json state` when live state is needed.*
