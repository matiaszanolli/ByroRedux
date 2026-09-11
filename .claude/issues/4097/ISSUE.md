# CHAR-2026-09-11-D2-02: `charal-fnv-fo3-ruleset.md` still repeats the three expired blocker claims that #3878 corrected in `stealth.rs`

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4097
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4097 --json state`.

---

Reported by `/audit-character` — see `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`).

- **Severity**: LOW
- **Dimension**: Derived Formulas (capture-document rot)
- **Game**: fnv, fo3
- **Location**: `docs/engine/charal-fnv-fo3-ruleset.md:331-336`
- **Source**: n/a (non-numeric — doc rot, not a constant)

## Description

Commit `4195ea41` ("Fix #3878: stealth.rs's deferral condition
  expired — record that, don't repeat it") establishes that all three claims in
  `stealth.rs`'s old status paragraph had expired, and rewrote that paragraph. Its diff
  touches **one file, 33 lines** (`crates/core/src/stealth.rs`). The capture document
  carries a near-verbatim copy of the same paragraph and was not touched, so it still
  reads: *"no ROADMAP milestone exists yet to consume it (M42 'AI packages' … is Tier 7
  and blocked on `PACK` record parsing, #446 … ECS wiring waits for M42)"*.

## Evidence

`git show --stat 4195ea41` → `crates/core/src/stealth.rs | 33 +++---`,
  one file. Document `:331-336` versus `stealth.rs:31-43`, which now states the
  opposite: "#446 is CLOSED (`90e6b068`) and `crates/plugin/src/esm/records/misc/pack.rs`
  is ~1,800 lines of shipped PACK parsing; an AI-package evaluator exists … M42 has
  delivered seven procedure runtimes". `git log --oneline -1 90e6b068` confirms
  #446 is closed.

## Impact

The capture documents are the designated authority for this layer, and a
  contributor picking up stealth wiring reads the document at least as often as the
  module docstring. The correction's own stated purpose — "a contributor reading this to
  pick up work should not be sent to wait on things that are done" — is defeated for
  exactly the readership the document serves. The fix left the more-read copy stale.

## Related

#3878 (CLOSED); #446 (CLOSED); D2-01 (same section)

## Suggested Fix

Port `stealth.rs:31-43`'s corrected paragraph into
  `charal-fnv-fo3-ruleset.md:331-336` — unscheduled, not blocked — and drop the
  "waits for M42" closer.

## Completeness Checks

- [ ] **SIBLING**: every other copy of this fact swept repo-wide (this class has repeatedly had one copy fixed and another missed — grep the symbol/number across `docs/`, `crates/`, `.claude/`, not just the named file)
- [ ] **TESTS**: where the doc states a pinned number or symbol, a test or `_audit-validate.sh` rule keeps it honest