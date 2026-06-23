# SCR-D4-02: Error recovery skips to EOF, not to the next line — silently truncates the rest of the script

Filed as: matiaszanolli/ByroRedux#1734
Source audit: `docs/audits/AUDIT_SCRIPTING_2026-06-23.md`

- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Pratt Parser
- **Location**: `crates/papyrus/src/parser/script.rs:625-633` (`skip_to_next_line`) × `mod.rs:71-81` (`peek` skips Newlines)
- **Labels**: medium, legacy-compat, bug

## Description
`skip_to_next_line` breaks on `Token::Newline`, but `peek()` skips all Newline tokens and never returns one, so the `Newline` arm is dead — the loop advances to EOF. After ONE recoverable top-level error, `parse_script` discards the rest of the file, defeating partial-success recovery. No test catches it.

## Suggested Fix
Use `peek_raw()` to find the line boundary; advance raw tokens until a raw `Token::Newline` is consumed. Add a two-item-first-malformed regression.
