# #5445: PEX-D4-2026-10-08-03: The type operand of `as` / `is` is pulled up from the next line with 0 errors — `parse_base_type` skips newlines (#5331 sibling, same root)

**Labels**: low,scripting,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5445

**Source**: `docs/audits/AUDIT_PAPYRUS_2026-10-08.md` — `PEX-D4-2026-10-08-03` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `parse_base_type` (`parser/mod.rs` ~425) still opens with `self.skip_newlines()`.

- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/mod.rs:425-426` (`parse_base_type` starts with
  `skip_newlines()`), reached from `parser/expr.rs:364-366` (`parse_cast`) and `:381-383` (`parse_is`)
- **Status**: NEW (sibling of open #5331)
- **Untrusted-Input**: Yes
- **Description**: #5021 moved each keyword's mandatory operand onto the raw token stream. `as`, and
  the new `is`, both have a mandatory type operand, but they reach it through `parse_type` →
  `parse_base_type`, which skips newlines first. #5331 reports this only for `New`, and its suggested
  fix is local to `parse_new_expr`. The shared cause is `parse_base_type`'s leading `skip_newlines`.
- **Evidence**: Probes, old and new parser alike (the `is` case only exists on the new one):
  - `Bool b = x is` ⏎ `Actor` → 1 item, **0 errors**
  - `Bool b = x as` ⏎ `Actor` → 1 item, **0 errors**
- **Impact**: The parser accepts source that the reference compiler rejects. The glued reading is what
  the author meant, so this is LOW, as with #5021 and #5331.
- **Suggested Fix**: Drop the leading `skip_newlines()` from `parse_base_type` and audit its callers.
  Alternatively, put `expect_same_line` before `parse_type` in `parse_cast`, `parse_is` and
  `parse_new_expr`, which fixes all three at once with #5331. Pin both probes in
  `a_mandatory_operand_on_the_next_line_is_an_error`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`parse_new_expr` (#5331) and other `parse_type` callers with a mandatory same-line operand)
- [ ] **TESTS**: A regression test pins this specific fix
