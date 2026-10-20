# TD2-2026-10-19-14: `obscript_runtime.rs` has two copies of the If/ElseIf/Else recursion (compiled lines vs source tokens)

Labels: low,scripting,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src/obscript_runtime.rs:242-317` (`parse_compiled_sequence` + `parse_compiled_conditional`) vs `:475-543` (`parse_source_sequence` + `parse_source_conditional`)
- **Age**: 2026-09-01 (aa0fa1b5d "lower ObScript elseif chains")
- **Effort**: small
- **Description**: `parse_*_conditional` are 35 identical lines (depth cap, then-branch, `match terminator { EndIf, Else, ElseIf, End }`, build `If`); the `*_sequence` loops differ only in how a line is classified. The depth cap `MAX_LEGACY_OBSCRIPT_NESTING` is enforced in both copies by hand.
- **Proposed fix**: one generic `fn parse_conditional<L>(parse_sequence: &mut impl FnMut(&mut usize, usize) -> Option<(Vec<Stmt>, StatementTerminator)>, cursor, depth, condition) -> Option<Stmt>` (the sequence closure captures `script`/`dialect`/`lines`); the two `*_conditional` functions disappear.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
