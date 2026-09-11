# CHAR-2026-09-11-D2-01: `DetectorState`'s state→multiplier assignment is unsourced — the same gap #3482 closed for `Sound`/`Visual`, left open one line above it

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4096
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4096 --json state`.

---

Reported by `/audit-character` — see `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`).

- **Severity**: LOW
- **Dimension**: Derived Formulas (CHARAL-adjacent sibling `stealth.rs`)
- **Game**: fnv, fo3
- **Location**: `crates/core/src/stealth.rs:156-176` (enum + `multiplier()`); document line `docs/engine/charal-fnv-fo3-ruleset.md:234`
- **Source**: UNSOURCED-in-code. `charal-fnv-fo3-ruleset.md:234` captures the *values*
  (`DetectorSkill = (10 + 8·Perception) × DetectorState  # 0.8 / 1.2 / 1 by AI state`)
  but no line anywhere in the CHARAL documents says which AI state carries which
  multiplier. `grep -niE "sleep|alert|fighting|DetectorState|AI state"` over the
  document returns only that line plus unrelated Hardcore-mode and prose hits.

## Description

`stealth.rs` ships three named detector states with a specific
  assignment, and its enum docstring asserts the semantics in prose — "Sleeping actors
  and actors already fighting their current target are *less* alert (0.8×); actors on
  edge (alert, lost, or fighting someone else) are *more* alert (1.2×)". None of that
  naming or assignment appears in any capture document. This is exactly the defect
  #3482 was filed for and exactly the class of statement its fix converted to captured
  source text for `Sound`/`Visual` — the pass simply stopped at the `DetectorSkill`
  line instead of continuing through it.

## Evidence

`stealth.rs:171-174`
  `SleepingOrFightingThisTarget => 0.8, AlertLostOrFightingOther => 1.2, Normal => 1.0`.
  The pinning test `detector_skill_and_state_match_the_source_table` (`:750-769`) asserts
  each pair against a literal in the test itself, so it pins the code to *itself*, not
  to a document — the same circularity the #3482 report identified for the other
  coefficients ("an attribution chain with nothing at the end of it").

## Impact

Swapping `0.8` and `1.2` would invert the effect of a detector being
  asleep versus alert, and nothing in the repository could catch it — the monotonicity
  test `alert_detector_state_raises_detection_over_normal` would fail, but only because
  it encodes the same unsourced assumption. No live consumer exists (zero callers), so
  today this is a correctness-of-record problem, not shipped wrong behaviour. It becomes
  gameplay-visible the moment M42 wires an alert-state tick.

## Related

#3482 (CLOSED — closed correctly for `Sound`/`Visual`, see the closure
  verdict above); #3878

## Suggested Fix

Re-read the same fandom *Sneak (Fallout: New Vegas)* `<math>` block
  the #3482 pass used and capture the three AI-state names beside their multipliers in
  `charal-fnv-fo3-ruleset.md`'s new sub-expression block, then cite that line from the
  enum. If the page names no states, say so explicitly and mark the assignment a
  disclosed modelling choice the way `SilentRunning`'s `VisualMovement` already is.

## Completeness Checks

- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix