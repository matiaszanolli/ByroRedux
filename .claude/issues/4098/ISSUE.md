# CHAR-2026-09-11-D2-03: `charal-skyrim-ruleset.md` still pins `DerivedStatFormula` at 32 bytes and names a test that no longer exists

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4098
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4098 --json state`.

---

Reported by `/audit-character` — see `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`).

- **Severity**: LOW
- **Dimension**: Derived Formulas (capture-document rot)
- **Game**: skyrim (the struct is `all`)
- **Location**: `docs/engine/charal-skyrim-ruleset.md:695-699`
- **Source**: n/a (non-numeric — a struct-contract claim, not a game constant)

## Description

The § Carry Weight build note says the `base_reads: u8` bitfield was
  "packed into the struct's one spare padding byte — so `size_of::<DerivedStatFormula>()`
  stays **exactly 32 bytes**, still enforced by `formula_is_thirty_two_bytes_and_copy`".
  The struct has been 36 bytes since #2939 added `floor: f32`, and
  `formula_is_thirty_two_bytes_and_copy` does not exist anywhere in the workspace — the
  live pin is `formula_is_thirty_six_bytes_and_copy` (`derived.rs:354`). #3485 fixed the
  SKILL.md copy of this claim (`8175cb70`) and `derived.rs`'s own docstrings were already
  correct; this third copy was not swept.

## Evidence

`grep -rn "thirty_two_bytes\|exactly 32 bytes" docs crates byroredux` →
  the only live-source hit is `docs/engine/charal-skyrim-ruleset.md:699` (the others are
  archived audit reports and unrelated Vulkan/GRAS byte counts).
  `git show 8175cb70 --stat` shows `.claude/commands/audit-character/SKILL.md` changed,
  this document not.

## Impact

This crate treats struct-size assertions as contracts (the pattern
  `resistance.rs:205-214` enforces mechanically, and #2954 was filed for exactly this
  divergence on `Affliction`). A contributor extending `DerivedStatFormula` from the
  Skyrim capture document is told a wrong budget and pointed at a test name that greps
  to nothing.

## Related

#3485 (CLOSED — SKILL.md copy fixed); #2939; #2954 (the same
  documented-size-vs-asserted-size divergence class)

## Suggested Fix

Change `:695-699` to "packed into a spare flag byte; the struct is
  36 bytes since #2939's `floor: f32`, pinned by `formula_is_thirty_six_bytes_and_copy`".

---

## Completeness Checks

- [ ] **SIBLING**: every other copy of this fact swept repo-wide (this class has repeatedly had one copy fixed and another missed — grep the symbol/number across `docs/`, `crates/`, `.claude/`, not just the named file)
- [ ] **TESTS**: where the doc states a pinned number or symbol, a test or `_audit-validate.sh` rule keeps it honest