# TD8-001: mswp::peek_path_filter is dead, reserved for a CLOSED issue (#584)

Issue: #1704 · Labels: low, import-pipeline, legacy-compat, tech-debt, bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-06-23.md

**Severity**: LOW
**Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
**Location**: `crates/plugin/src/esm/records/mswp.rs` (`peek_path_filter`)

## Description
`peek_path_filter` carries `#[allow(dead_code)] // Reserved for the FO4-DIM6-02 stage-2 cell-loader integration.` It has **zero callers** anywhere in the workspace, and FO4-DIM6-02 (#584, "TXST.MNAM parsed but never resolved at REFR time") is **CLOSED**. The reservation breadcrumb has outlived its driver.

## Evidence
- `grep -rn 'peek_path_filter' --include='*.rs'` returns only the definition (`mswp.rs:151`).
- `gh issue view 584` → state `CLOSED`.

## Impact
Pure rot — a `pub(crate) fn` no one consumes, with a stale forward-reference to a closed issue. CLAUDE.md: delete, no breadcrumbs.

## Suggested Fix
Delete the function and its reservation comment. If MSWP path-filter peeking is genuinely needed by a *future* cell-loader path, re-add it at that call site under a *live* issue.

## Completeness Checks
- [ ] **SIBLING**: No other `#[allow(dead_code)]` breadcrumbs in `mswp.rs` reference the same closed driver
- [ ] **TESTS**: The `mswp` wire-format tests still pass after removal (none exercise `peek_path_filter`)
