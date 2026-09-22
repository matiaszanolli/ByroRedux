# TD1-2026-09-22-02: storage_util.rs (2320 LOC) tech debt still open — tracking issue #4218 closed 2026-09-16 without the split landing

**Labels**: low, tech-debt, documentation, doc-rot

Filed via /audit-publish from docs/audits/AUDIT_TECH_DEBT_2026-09-22.md.

**Severity**: LOW · **Dimension**: 1 — File / Function / Module Complexity
**Location**: `crates/sdk/src/compatibility/storage_util.rs:596` (`papyrus_storage_util_declarations`, 251-line vec), `:2106` (`adapt_storage_util_global_list`, 384-line/17-arm dispatcher)

**Status**: `#4218` is CLOSED (2026-09-16) but unresolved — the split it describes never landed; filed fresh since the issue no longer tracks this as open.
**Verified against**: HEAD `c3f298a24` — re-ran `prod_loc` live (2320, unchanged), confirmed both functions at their original line numbers, confirmed `#4218` state CLOSED via `gh issue view`.

## Description

No commit touches this file anywhere in the delta window; both functions remain verbatim at their original line ranges. The 09-21 tech-debt report itself listed this as "Existing: #4218 (OPEN)" — but `#4218` had already been closed five days earlier (2026-09-16), meaning that report's dedup check was already stale at publish time. This is not a regression (the split was never applied, so nothing broke); it is the same unresolved debt under a closed-and-therefore-untracked issue.

## Impact

No runtime impact — pure maintainability. The live gap is bookkeeping: real unresolved debt with no OPEN issue tracking it.

## Related

#4218 (closed without the fix), the 09-21 tech-debt report (whose dedup check was stale at publish for this specific item — process awareness note, not itself refiled).

## Suggested Fix

Unchanged from #4218's own proposal: extract each `adapt_storage_util_global_list` arm into a named helper behind a thin dispatcher; replace `papyrus_storage_util_declarations`'s vec literal with a `const` table. Re-filed as a fresh issue since #4218 no longer represents open work.

Source: docs/audits/AUDIT_TECH_DEBT_2026-09-22.md (TD1-2026-09-22-02)
