# REG-2026-08-16-D5-02: run_skinning_invariant asserts nothing

**Issue**: #3083
**Severity**: MEDIUM
**Labels**: `medium,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_REGRESSION_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_REGRESSION_2026-08-16.md` (Dimension 5 — Green-by-construction guard sweep).

**Location**: `byroredux/tests/skinning_e2e.rs`:188 (`run_skinning_invariant`)

## Description

`run_skinning_invariant` **asserts nothing** — three `_check` tests in `skinning_e2e.rs` are `eprintln!`-only, in a file whose own doc claims *"no soft flags"*.

## Evidence

Re-verified 2026-08-17:
- `byroredux/tests/skinning_e2e.rs`:188 defines `run_skinning_invariant`; its body contains **zero** `assert!`/`assert_eq!`
- The file contains 5 `_check` references
- `skinning_e2e.rs`:21 states: *"regressions, no soft flags."*

The file's stated contract and its actual behaviour are opposites: it advertises hard assertions and delivers diagnostic prints.

## Impact

The skinning end-to-end guard cannot fail. Any regression in the skinning invariants it names passes as green, and the doc comment actively discourages a reader from checking — it says the file has no soft flags.

This is the archetype of the green-by-construction class the tech-debt sweep's Dimension 9 was extended to find (#2983, #3014, #3017 are siblings).

## Suggested Fix

Convert the `eprintln!` diagnostics to assertions, or — if the invariants are genuinely not assertable yet — correct the file docstring so it stops claiming otherwise, and mark the tests as diagnostics rather than guards.

Silently-passing is the part to remove; either direction fixes it.

## Related

- #2983, #3014, #3017 (the same green-by-construction class this sweep)
- #3081 (REG-D5-01 — the other one-directional guard in this dimension)

## Completeness Checks
- [ ] **ASSERTS-OR-HONEST-DOC**: Either the checks assert, or the "no soft flags" claim is removed
- [ ] **ALL-THREE**: All three `_check` tests addressed, not just one
- [ ] **FAILS-LOUDLY**: Breaking a skinning invariant turns the file red
- [ ] **TESTS**: A deliberately broken invariant fails the suite

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3083 --json state` when live state is needed.*
