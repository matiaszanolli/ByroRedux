# SPT-D2-2026-08-16-02: import/mod.rs docstring documents the pre-#1001/#1002 size precedence

**Issue**: #3080
**Severity**: LOW
**Labels**: `low,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_SPEEDTREE_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SPEEDTREE_2026-08-16.md` (Dimension 2 — TLV walk).

**Location**: `crates/spt/src/import/mod.rs`:22-23

## Description

`import/mod.rs`'s module docstring still documents the **pre-#1001/#1002 two-tier size precedence**, which those two changes replaced.

## Impact

Doc rot on the module's orientation comment. Low cost in isolation, but `crates/spt` is a small crate whose docstring is the main explanation of how sizing works — a reader has little else to go on.

## Suggested Fix

Refresh the docstring to the current precedence rule as implemented after #1001/#1002.

## Related

- #1001, #1002 (the changes that superseded the documented rule)
- #3079 (SPT-D3-02 — the skill-side location drift for the same crate)

## Completeness Checks
- [ ] **CURRENT-RULE**: The docstring matches the post-#1001/#1002 implementation
- [ ] **SIBLING**: Other `crates/spt` module docs checked for the same lag
- [ ] **PATH-GATE**: `_audit-validate.sh` still passes

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3080 --json state` when live state is needed.*
