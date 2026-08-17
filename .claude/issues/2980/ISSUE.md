# TD3-2026-08-16-03: combat_input_system's comment says the damage is re-read from the trace; the consumer recomputes it

**Issue**: #2980
**Severity**: LOW
**Dimension**: 3 — Stale Documentation & Comments
**Labels**: `low,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md` (Dimension 3 — Stale Documentation & Comments). Effort: trivial.

**Location**: `byroredux/src/combat.rs`:159-163, against :203-207 and :269-273
**Age**: `eb5d76fe`, 2026-08-16

## Description

The comment justifying `source: aggressor` ends:

> damage was snapshotted into the trace and is re-read same-frame by the consumer.

**No such read happens.** `combat_input_system` computes `damage` at :150, stores it in `CombatTraceEntry.damage`, and `combat_damage_system` then calls `attack_damage(world, event.aggressor)` **again** at :206 and overwrites the trace entry wholesale at :245-252. The value is computed twice from the same source and the first copy is discarded.

Harmless today because both calls run in the same frame against an unchanged `EquippedWeapon` — but the comment documents a data-flow contract the code does not implement, which is precisely what makes a future `EquippedWeapon` writer (ECS-2026-08-16-04's suggested fix) a **silent divergence** rather than a caught one.

## Evidence

```rust
// combat.rs:159-163
// Equipped weapons are inventory records rather than standalone
// ECS entities today. Use the aggressor as the source until item
// instances acquire stable entities; damage was snapshotted into
// the trace and is re-read same-frame by the consumer.
source: aggressor,
```

```rust
// combat.rs:203-207 — the "consumer"
let damage = if event.blocked { 0.0 } else { attack_damage(world, event.aggressor) };
```

`CombatState.last.damage` has exactly one reader: `commands/view.rs`'s `combat.status` formatter.

## Impact

Documentation only, but **load-bearing for the next edit to this file** — the two most likely near-term changes (a runtime `EquippedWeapon` writer, and routing damage through CHARAL) both turn "computed twice" into a correctness question.

## Suggested Fix

Either:

1. Add a `damage: f32` field to `HitEvent` and have the consumer read it (which also fixes the scripted-producer case), **or**
2. Correct the comment to say the consumer recomputes.

Option 1 is the better fix if a scripted `HitEvent` producer is coming, since a script-authored hit has no `EquippedWeapon` to recompute from.

## Related

- #2976 (TD6-2026-08-16-01 — the `blocked` field in the same struct)
- `AUDIT_ECS_2026-08-16` § ECS-2026-08-16-04 (the `EquippedWeapon` write-path gap)
- `AUDIT_CHARACTER_2026-08-16` § CHAR-2026-08-16-D1-01

## Completeness Checks
- [ ] **SIBLING**: If `HitEvent` gains a `damage` field, every producer (incl. tests) sets it
- [ ] **SINGLE-SOURCE**: Damage computed once, not twice, if option 1 is taken
- [ ] **COMMENT-TRUTH**: The comment matches the code whichever option is chosen
- [ ] **TESTS**: A regression test pins the producer→consumer damage path

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2980 --json state` when live state is needed.*
