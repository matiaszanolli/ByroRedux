# REG-2026-08-16-D1-01: #2955's fix was copy-pasted into inventory.rs; only the original is guarded

**Issue**: #3081
**Severity**: MEDIUM
**Labels**: `medium,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_REGRESSION_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_REGRESSION_2026-08-16.md` (Dimension — Closed-issue discovery & fix presence).

**Location**: `byroredux/src/inventory.rs`:179-185 (the duplicate) · `byroredux/src/npc_spawn.rs`:131-137 (the fixed original) · `byroredux/src/npc_spawn/tests.rs`:891 (the guard that reaches only the original)

## Description

#2955 (HIGH, closed 2026-08-15 by `4f1eb7dd`) established that an `NPC_`'s ACBS `level` field is a **PC-level multiplier**, not a level, when `ACBS_PC_LEVEL_MULT` is set — and routed every numeric reader through one `effective_actor_level` helper.

The next day, `09682c71` added `byroredux/src/inventory.rs` with a **second, private `effective_actor_level`** rather than importing the fixed one.

## Evidence

Two definitions, re-verified 2026-08-17:

```rust
// byroredux/src/npc_spawn.rs:131 — the #2955 fix, guarded
fn effective_actor_level(npc: &byroredux_plugin::esm::records::NpcRecord) -> i16 {
    if npc.acbs_flags & byroredux_plugin::esm::records::ACBS_PC_LEVEL_MULT != 0 {
        npc.calc_min.max(1) as i16
    } else {
        npc.level.max(0)      // <- clamp 0
    }
}

// byroredux/src/inventory.rs:179 — the copy, unguarded
fn effective_actor_level(actor: &NpcRecord) -> i16 {
    if actor.acbs_flags & ACBS_PC_LEVEL_MULT != 0 {
        actor.calc_min.max(1) as i16
    } else {
        actor.level.max(1)    // <- clamp 1 — ALREADY DIVERGED
    }
}
```

Both currently implement #2955's `calc_min` branch correctly, so this is **not a code regression today** — but the two have **already diverged on their clamp**, one day after the copy was made.

The #2955 regression test `pc_level_mult_actors_resolve_to_calc_min_not_the_raw_multiplier` lives at `byroredux/src/npc_spawn/tests.rs`:891 and calls the `npc_spawn` copy exclusively. **Nothing in the workspace can detect the `inventory.rs` copy losing the `calc_min` branch.**

## Impact

A closed HIGH fix exists in two places with one guard. The divergence already present in the clamp is the proof of concept: the copies drift, and only one drift is observable.

Per the project's "improve existing code rather than duplicating logic" rule, the duplication is the defect regardless of current behavioural agreement.

## Suggested Fix

Delete the `inventory.rs` copy and import the `npc_spawn` one (or hoist the helper to a shared module). Resolve the clamp divergence deliberately — `max(0)` and `max(1)` cannot both be right — and state which is correct.

## Related

- #2955 (CLOSED — the fix that was copy-pasted)
- #3032 (ECS-2026-08-16-04 — the other `inventory.rs`/spawn-path divergence in the same slice)

## Completeness Checks
- [ ] **NO-DUPLICATION**: One `effective_actor_level`, imported not copied
- [ ] **CLAMP-RESOLVED**: The `max(0)` vs `max(1)` divergence decided deliberately and documented
- [ ] **GUARD-REACHES**: The #2955 regression test exercises the single surviving implementation
- [ ] **SIBLING**: Other helpers added by `09682c71` checked for the same copy-instead-of-import pattern
- [ ] **TESTS**: A regression test pins this specific fix

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3081 --json state` when live state is needed.*
