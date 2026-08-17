# TD3-2026-08-16-02: crates/core/src/combat.rs still asserts no combat consumer exists in the engine — one shipped a day earlier

**Issue**: #2979
**Severity**: LOW
**Dimension**: 3 — Stale Documentation & Comments
**Labels**: `low,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md` (Dimension 3 — Stale Documentation & Comments). Effort: trivial.

**Location**: `crates/core/src/combat.rs`:10-11
**Age**: docstring `9cf93368`, 2026-07-04; falsified `eb5d76fe`, 2026-08-16

## Description

The module docstring reads:

> No combat/attack-resolution consumer system exists yet in the engine; this module is the reusable, tested piece built ahead of that consumer.

`byroredux/src/combat.rs` + `combat_input_system` + `combat_damage_system` — a complete ray → hit → damage → death pipeline registered as two `Stage::Update` exclusives in `byroredux/src/boot.rs`:780-781 — **is exactly that consumer**, and landed 2026-08-15/16. It simply took a different path.

The docstring's claim is the *reason* the module's zero-caller state reads as deliberate rather than as a gap, so leaving it in place converts a real design question into a documented non-issue.

## Evidence

`crates/core/src/combat.rs`:10-11 (quoted above).

Workspace grep for `modified_skill` / `oblivion_weapon_damage_multiplier` / `oblivion_hand_to_hand_damage` / `byroredux_core::combat` outside the file itself returns **zero** hits.

`byroredux/src/combat.rs` does not import `byroredux_core::character` or `byroredux_core::combat` at all; its damage model is `EquippedWeapon.damage` or the flat `UNARMED_DAMAGE = 8.0`.

`crates/core/src/stealth.rs` (487 LOC) is in the same state, and its `sneak_attack` counterpart is hardcoded `false` at the `HitEvent` producer.

## Impact

Doc rot only — the correctness half is already owned by `AUDIT_CHARACTER_2026-08-16` § CHAR-2026-08-16-D1-01.

But this is the sentence that will keep the next reader from asking why two combat-math modules exist unconnected beside a third that *is* connected and uses neither.

## Suggested Fix

Reword to name the live consumer and state plainly that it does not route through this module yet, with a pointer to #2962 / CHAR-2026-08-16-D1-01.

Same for `crates/core/src/stealth.rs` if it carries an equivalent claim.

## Related

- `AUDIT_CHARACTER_2026-08-16` § CHAR-2026-08-16-D1-01 (same sweep — the consumer bypasses CHARAL; names the zero-caller state but not this docstring)
- #2962 (OPEN — ownership of `crates/core/src/combat.rs` and `stealth.rs`)
- #2976 (the `Block` stub in the live consumer)

## Completeness Checks
- [ ] **SIBLING**: `crates/core/src/stealth.rs` checked for the same "no consumer exists yet" claim and corrected if present
- [ ] **POINTER**: The reworded docstring names the live consumer and the open design question, not just "a consumer exists"
- [ ] **NO-SCOPE-CREEP**: This is the doc fix only — the CHARAL routing question stays with #2962

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2979 --json state` when live state is needed.*
