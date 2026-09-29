# PEX-D4-2026-09-29-01: A `;` line comment in a CR-only file swallows the rest of the file silently (#4479's CR fix left `skip_line_comment` on `
` only)

**Labels**: medium,bug,scripting

**Source report**: `docs/audits/AUDIT_PAPYRUS_2026-09-29.md`

- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/token.rs:5-14` (`skip_line_comment`)
- **Status**: NEW (left over from closed #4479, PEX-D4-2026-09-19-03)
- **Untrusted-Input**: Yes
- **Description**: #4479 made `\r\n|\r|\n` the `Newline` token, so a lone CR terminates a line. The
  line-comment callback still finds the end of the comment with `remainder.find('\n')`. A CR-only
  file has no `\n`, so the first `;` comment bumps the lexer to EOF, and every later line is
  discarded. CRLF files are unaffected: the `\r` is absorbed into the comment and `\n` still lexes as
  `Newline`.
- **Evidence**: Probe results from a scratch crate with a path dependency on `byroredux-papyrus`,
  calling `parse_script`:

  | Input | items | errors |
  |---|---|---|
  | `ScriptName Foo⏎; header comment⏎Function F()⏎EndFunction⏎Function G()⏎EndFunction⏎`, LF | 2 | 0 |
  | Same input, CRLF | 2 | 0 |
  | Same input, **CR-only** | **0** | **0** |
  | CR-only, no comment | 2 | 0 |
  | CR-only `Int x = 5 ; trailing⏎Function F()…` | 1 (F dropped) | 0 |
  | CR-only comment inside a function body | 0 | 1 |

  The #4479 regression test `bare_cr_is_a_newline_and_crlf_is_one_newline` has no comment in its
  fixture, which is why the suite stays green.
- **Impact**: On a CR-only `.psc`, every function, event and property after the first comment
  disappears from the AST. Recovery reports nothing when the comment sits between top-level items.
  #4479 rated the gluing variant of this input class LOW. This variant drops whole items instead of
  gluing statements, and it defeats #4479's fix on essentially every real CR-only script.
  Reachability is the same bracket as #4472/#4763: the offline `extender_preflight` `scan_source` and
  the debug-console `parse_expr`. No runtime or game-load path parses `.psc`.
- **Related**: #4479 (closed, incomplete), #4763
- **Suggested Fix**: End the comment at the first `\r` or `\n` (`remainder.find(['\r', '\n'])`), leaving
  the terminator for the `Newline` rule. Add a CR-only-plus-comment case to the #4479 test.

**Validated at HEAD 9fcfdc3fc**: `skip_line_comment` in `crates/papyrus/src/token.rs` still ends the comment with `remainder.find('\n')`, so a CR-only file's first `;` comment bumps to EOF. Related to closed #4479 (its CR fix is left incomplete by this).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
