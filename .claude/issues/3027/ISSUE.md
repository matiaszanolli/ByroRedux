# SAVE-D1-02: ActorVitals excluded from MUTABLE_DELTA_COLUMNS with no recorded reason

**Issue**: #3027
**Severity**: LOW
**Dimension**: 1 — snapshot/delta model
**Labels**: `low,ecs,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_SAVE_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SAVE_2026-08-16.md` (Dimension 1 — snapshot/delta model).

**Location**: `byroredux/src/save_io.rs`:249 (registration, no comment), :83-129 (`MUTABLE_DELTA_COLUMNS`, absent) · `byroredux/src/save_io/round_trip_tests.rs`:737-748 (`NPC_SPAWN_STAMPED`, absent) · `byroredux/src/npc_spawn.rs`:112 (the stamp site)

## Description

`ActorVitals` is the **only registered component excluded from `MUTABLE_DELTA_COLUMNS` with no recorded reason**, and it is simultaneously missing from the `NPC_SPAWN_STAMPED` guard list despite being stamped at spawn.

## Evidence

Re-verified 2026-08-17:
- `sed -n '83,129p' byroredux/src/save_io.rs | grep -c ActorVitals` → **0**
- Registered at `save_io.rs`:249 with no explanatory comment
- Stamped at `npc_spawn.rs`:112, but absent from `NPC_SPAWN_STAMPED` (`round_trip_tests.rs`:737-748)

Every other registered component either appears in `MUTABLE_DELTA_COLUMNS` or carries a comment saying why not.

## Impact

Low on its own — but the exclusion is **indistinguishable from an oversight**, which is the same legibility problem as #2990. `ActorVitals` carries actor health, which the P2 combat slice mutates every swing, so "is this deliberately not a mutable delta column?" is a question a reader should not have to guess at.

If the exclusion *is* accidental, mid-session health changes do not survive a reload.

## Suggested Fix

Determine whether `ActorVitals` should be a mutable delta column. If yes, add it. If no, record the reason inline — and add it to `NPC_SPAWN_STAMPED` either way, since it is demonstrably stamped at spawn.

## Related

- #2990 (ESM-2026-08-16-D4-01 — the same omission-vs-intent legibility problem)
- #3022 (SAVE-D1-2026-08-16-01 — the other delta-model finding in this dimension)

## Completeness Checks
- [ ] **LEGIBLE-INTENT**: Whichever way it goes, the decision is recorded so it cannot be re-litigated
- [ ] **GUARD-LIST**: `NPC_SPAWN_STAMPED` includes it regardless — it is stamped at spawn
- [ ] **COMBAT**: Verify whether mid-session health survives a reload today
- [ ] **TESTS**: A round-trip test covers a mid-session `ActorVitals` change

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3027 --json state` when live state is needed.*
