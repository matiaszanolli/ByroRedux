# #5331 — PEX-D4-2026-10-05-04: parse_new_expr still skips newlines — `New` ⏎ `Int[5]` and `New Int` ⏎ `[5]` glue with 0 errors (#5021 sibling)

- **Labels**: low,scripting,bug
- **Filed from**: `docs/audits/AUDIT_PAPYRUS_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5331

- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/expr.rs:237` (`parse_base_type`, which calls
  `skip_newlines`) and `:238` (`expect(&Token::LBracket, …)`)
- **Status**: NEW (sibling of closed #5021)
- **Untrusted-Input**: Yes
- **Description**: #5021 moved every keyword with a *mandatory operand* onto the raw stream.
  `New` is a prefix keyword with a mandatory type and `[`, but it still goes through the
  newline-skipping helpers.
- **Evidence**: These probes produce `Int[] a = New Int[5]` with 0 errors:
  - `Int[] a = New Int` ⏎ `[5]`
  - `Int[] a = New` ⏎ `Int[5]`
- **Impact**: The parser accepts source that the reference compiler rejects. As with #5021, the
  glued form is what the author evidently meant, so this is LOW.
- **Suggested Fix**: Call `expect_same_line` before `parse_base_type` and use `expect_raw` for the `[`.
  Add both probes to `a_mandatory_operand_on_the_next_line_is_an_error`.

_Source: `AUDIT_PAPYRUS_2026-10-05.md` (PEX-D4-2026-10-05-04), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
