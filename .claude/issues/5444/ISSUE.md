# #5444: PEX-D4-2026-10-08-02: `skip_to_line_end` (#5322 recovery) consumes `EndStruct` / `EndGroup`, so a glued container terminator swallows the rest of the file

**Labels**: low,scripting,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5444

**Source**: `docs/audits/AUDIT_PAPYRUS_2026-10-08.md` — `PEX-D4-2026-10-08-02` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: the `skip_to_line_end` un-consume arm (`parser/mod.rs` ~292-299) still omits `KwEndStruct` / `KwEndGroup`.

- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/mod.rs:288-306`
- **Status**: NEW (side gap of the #5322 fix)
- **Untrusted-Input**: Yes
- **Description**: The un-consume list covers `Else`, `ElseIf`, `EndIf`, `EndWhile`, `EndEvent`,
  `EndFunction`, `EndProperty` and `EndState`. It omits `KwEndStruct` and `KwEndGroup`, even though
  the `expect_eol` doc and the commit message both promise that `End*` terminators are left in place.
  The struct and group member loops (`script.rs:694`, `:741`) call `expect_eol`. On a glued
  terminator, recovery eats it, so the container stays open until EOF.
- **Evidence**: A/B probe, `ScriptName Foo` ⏎ `Struct S` ⏎ `Int a = 1 EndStruct` ⏎ `Function F()` ⏎
  `EndFunction`:
  - Old parser: 2 items, 0 errors.
  - New parser: **1 item, 4 errors** ("type, found Function", …, "UnexpectedEof EndStruct"). `F` is
    lost.

  The same happens with `Int Property A Auto EndGroup`.
- **Impact**: This source is invalid Papyrus, so errors are correct. But the error cascade drops valid
  items that follow, which every other terminator avoids.
- **Suggested Fix**: Add `Token::KwEndStruct | Token::KwEndGroup` to the un-consume arm, and add the
  struct and group shapes to `glued_statement_tail_is_an_error_and_not_a_statement`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every other `End*` terminator the parser recognises)
- [ ] **TESTS**: A regression test pins this specific fix
