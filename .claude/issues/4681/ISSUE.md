# CHAR-2026-09-21-D2-01: fallout.rs module docstring still states Crit/Melee/Unarmed are actor-general as fact — #4450 fix only edited the function doc

**Severity**: LOW
**Dimension**: Derived Formulas
**Game**: FO3 / FNV

## Description

The module doc says "Carry Weight / Melee Damage / Critical Chance / Unarmed Damage are actor-general. That justification is sourced for Health ... but **not** for FO3/FNV Action Points". It presents the three unsourced scopes as settled, and lists AP as the only unsourced exception. This is the overstatement #4450 was filed for. The function doc and the capture were fixed; the module summary a reader meets first was not.

## Evidence

Verified at HEAD `ee6d3fb39`. `crates/core/src/character/fallout.rs` module doc: "Health / Action Points are flagged `player_only` ... Carry Weight / Melee Damage / Critical Chance / Unarmed Damage are actor-general. That justification is sourced for Health (every game) and for FO4/FO76 Action Points ... but **not** for FO3/FNV Action Points, which is `player_only` as a conservative, unsourced choice (#2937)."

This contradicts the function doc on `add_fnv_fo3_shared` in the same file: "Critical Chance, Melee Damage and Unarmed Damage ship `ActorGeneral` as an explicit but UNsourced choice — no capture line states their scope (#4450), the mirror of #2937's documented conservative `player_only` for Action Points."

## Impact

Doc only; misleads a reader about which scopes are sourced.

## Related

#4450 (CLOSED, fixed the function doc and the capture), #2937.

## Suggested Fix

Add to the module doc: "Critical Chance / Melee Damage / Unarmed Damage are an explicit, unsourced `ActorGeneral` choice (#4450, pinned by `fo3_fnv_crit_melee_unarmed_scopes_are_actor_general_pending_a_source`)".

Source: docs/audits/AUDIT_CHARACTER_2026-09-21.md (CHAR-2026-09-21-D2-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
