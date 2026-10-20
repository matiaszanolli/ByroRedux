# TD4-2026-10-19-03: `_audit-owners.md:70` owns `fragment/{vm,budget,reference}.rs`, so the split `vm/` tree (~3.8k prod LOC, the hottest file family of the last week) routes to `scripting` alone at MEDIUM

Labels: medium,documentation,doc-rot,tech-debt
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: MEDIUM | **Dimension**: 4 | **Effort**: trivial | **Age**: `ef3dafe38` (10-16)
- **Evidence**: `_audit-route.sh` output: `fragment/vm.rs` -> `HIGH scripting, concurrency, performance Dim 1, tech-debt Dim 1`; `fragment/vm/interp.rs`, `fragment/vm/checker.rs`, `translate/effects/lower.rs` -> `MEDIUM scripting`. `/audit-incremental` and `/audit-suite --changed|--area` therefore drop the concurrency, performance and tech-debt owners and the HIGH floor for the interpreter, whose own audit text calls out lock/budget/allocation risk (`audit-scripting/SKILL.md:282,710`).
- **Proposed fix**: change the row to `crates/scripting/src/fragment/{vm,budget,reference}` (stem form, as `byroredux/src/save_io` at line 149 already is); add `crates/scripting/src/translate/effects` to the scripting row if it should carry a floor. Re-run `_audit-route.sh` on `git ls-files crates/scripting/src/fragment/vm` to confirm HIGH.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in the neighbouring docs / skills
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
