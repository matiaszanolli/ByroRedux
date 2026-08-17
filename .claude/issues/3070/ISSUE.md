# SKY-2026-08-16-D1-01: BSXFlags bit-5 carve-out comment places Skyrim on the wrong side of its predicate

**Issue**: #3070
**Severity**: MEDIUM
**Labels**: `medium,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_SKYRIM_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SKYRIM_2026-08-16.md` (Dimension 1 — cell loading).

**Location**: `byroredux/src/cell_loader/references/import.rs`:68-79 (comment) vs :89 (code) · sibling at `byroredux/src/cell_loader/partial.rs`:55-66

## Description

The `BSXFlags` bit-5 carve-out **comment places Skyrim on the wrong side of its own predicate**.

The code gates on `bsver < FALLOUT4`, which *includes* Skyrim in the drop. The comment above it describes the carve-out as if Skyrim were excluded.

## Impact

Documentation-vs-code divergence on a rule that silently discards whole NIFs (#3036 shows it deletes 223 real FNV meshes). A reader checking whether Skyrim is affected gets the wrong answer from the comment and would have to read the predicate to find out.

Since #3036 proposes changing this rule, the misleading comment is exactly what a fixer would read first.

## Suggested Fix

Correct the comment to match the predicate — or, better, fix them together with #3036, since that finding replaces the whole-NIF drop with marker-child culling and both comment sites become obsolete.

## Related

- **#3036 (FNV-D1-01) — the substantive bug in the same predicate; fix together**
- The sibling copy at `partial.rs`:55-66 has the same comment

## Completeness Checks
- [ ] **BOTH-SITES**: `references/import.rs` and `partial.rs` comments both corrected — the rule is duplicated
- [ ] **CO-RESOLVE**: Fixed with #3036 rather than separately
- [ ] **COMMENT-TRUTH**: The comment states which games the predicate actually covers

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3070 --json state` when live state is needed.*
