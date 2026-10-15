# TD3-2026-10-15-08: ci.yml says the renderer has "733 `unsafe` blocks"; it has 704

Labels: low,safety,tech-debt,documentation,doc-rot

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD3-2026-10-15-08`).

- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `.github/workflows/ci.yml:211`
- **Status**: NEW
- **Age**: `530c9e7aa` (2026-09-30)
- **Effort**: trivial
- **Description**: `grep -rE "unsafe *\{" crates/renderer/src --include='*.rs' | wc -l` = 704 (no commented-out hits). The helper consolidation (#5593 etc.) shrank it.
- **Evidence**: command above; `sed -n 211p .github/workflows/ci.yml`.
- **Impact**: none functional; a hand-typed count in a CI comment will drift again.
- **Related**: #5121, #4595.
- **Suggested Fix**: drop the number ("the renderer's `unsafe` blocks").

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
