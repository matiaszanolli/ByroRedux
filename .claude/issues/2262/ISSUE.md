# TD4-002: Tech-debt skill's own Phase-1 #[ignore]-count baseline recipe scans the whole repo textually, producing a false ~2.4x 'regression' signal

Severity: low
Source audit: docs/audits/AUDIT_TECH_DEBT_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2262

**Dimension**: 4 (Audit-Finding Rot)
**Location**: `.claude/commands/audit-tech-debt/SKILL.md` (Phase 1 baseline snapshot; Dimension 9 discovery command) — both use `grep -RIn '#\[ignore\]' .` (or with only `| grep -v target/`) with no `--include='*.rs'` file-type filter
**Status**: NEW

**Description**: The Phase-1 baseline recipe scans every tracked file, not just `.rs` sources — it picks up every prose mention of the literal string `#[ignore]` inside markdown, including this very skill file's own Dimension-9 discovery line, and every prior `docs/audits/*.md` tech-debt report that quotes an `#[ignore]` count in its own baseline snapshot section (a self-reinforcing inflation: each report that prints the raw count adds another textual hit for the next report's raw count).

**Evidence**: Raw repo-wide (the skill's literal recipe): 323. Scoped to `.rs` files, `target/` excluded (doc-comment + attribute mentions): 135. Scoped to actual `#[ignore]` attribute lines only: 96. The 07-25 report's baseline snapshot quotes "`#[ignore] tests: 135`" (the correctly-scoped figure) — but the skill's own committed recipe, if re-run literally today, returns 323, which would read as a "+139% regression" against that baseline even though the real (scoped) count has moved only marginally.

**Impact**: Any future run of this audit's own Phase-1 snapshot (or Dimension 9's discovery step) that takes the number at face value would misdiagnose a large `#[ignore]`-test debt spike that doesn't exist — the entire delta is markdown noise. This is exactly the audit-finding-rot failure mode Dimension 4 exists to catch, except the rot is in the audit tool's own measurement, not in a stale path/symbol.

**Suggested Fix**: Scope both greps to `--include='*.rs'` and exclude `target/`, matching the two other Phase-1 metrics in the same block (both already scoped to `crates byroredux`). Recommended replacement: `grep -RIn '^\s*#\[ignore\]' --include='*.rs' crates byroredux | wc -l` (actual attribute lines only, consistent with how the other two baseline metrics are already directory-scoped).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix, if applicable
