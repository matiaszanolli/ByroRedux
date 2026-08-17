# CONC-2026-08-16-03: pre_parse_cell's doc comment documents the wrong function

**Issue**: #3091
**Severity**: LOW
**Labels**: `low,sync,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_CONCURRENCY_2026-08-16.md` (Dimension — Worker Threads, documentation).

**Location**: `byroredux/src/streaming.rs` (`pre_parse_cell`'s doc comment)

## Description

`pre_parse_cell`'s doc comment was **cut in half by the #1262 extraction** and now documents the wrong function — the surviving half describes behaviour that moved to the extracted callee.

## Impact

Documentation rot on the M40 streaming path's entry point. A reader consulting the comment to understand what `pre_parse_cell` does gets a description of a different function's work.

Relevant now because #3089 proposes changing how that path's parallel phase is scheduled — the misleading comment is what a fixer reads first.

## Suggested Fix

Rewrite the comment to describe `pre_parse_cell` as it now stands, and move the extracted half onto the function #1262 created (or delete it if that function documents itself).

## Related

- #1262 (the extraction that split the comment)
- #3089 (CONC-01 — the same function's rayon contention; fix the comment while there)

## Completeness Checks
- [ ] **RIGHT-FUNCTION**: The comment describes `pre_parse_cell`'s actual behaviour
- [ ] **EXTRACTED-HALF**: The moved behaviour is documented on the function that now performs it
- [ ] **SIBLING**: Other #1262-era extractions in `streaming.rs` checked for split comments
- [ ] **PATH-GATE**: `_audit-validate.sh` still passes

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3091 --json state` when live state is needed.*
