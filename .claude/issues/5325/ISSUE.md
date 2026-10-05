# #5325 — PEX-D4-2026-10-05-02: Papyrus doc comments are only collected before a declaration, but Papyrus puts them after — a bodied function/event with a doc comment is dropped and its body hoisted into script scope

- **Labels**: medium,scripting,bug
- **Filed from**: `docs/audits/AUDIT_PAPYRUS_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5325

- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/script.rs:286` (`parse_function`), `:318` (`parse_event`),
  `:397` (`parse_property`). Each calls `skip_newlines_collect_doc()` *before* its keyword. Nothing
  consumes a `DocComment` after the header line, so `parse_block` reaches it as the first statement.
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: The falloutck wiki (*Script File Structure*, §Documentation Comments) says doc
  comments "can only appear on the line following a script header, property definition, group
  definition, struct member definition, or function definition". The `.pex` `doc_string` fields are
  written from that position. The parser instead attaches the `{…}` that comes *before* an item.
  The result depends on the item:
  - **Native function or property:** the doc after the header is lost, and `doc_comment` is `None`.
  - **Function or event with a body:** the `DocComment` becomes the first body statement, which
    raises "expected expression, found doc comment". The item is dropped, and script-level
    recovery re-parses its body lines as top-level items.
- **Evidence**:

  | Input | Result | errors |
  |---|---|---|
  | `Function F()` ⏎ `{doc for F}` ⏎ `Int x = 1` ⏎ `EndFunction` ⏎ `Function G()…` | items `[Var x, Fn G]`: F is gone, and its local `x` became a **script variable** | 2 |
  | `Event OnInit()` ⏎ `{doc}` ⏎ `EndEvent` | items `[]` | 2 |
  | `Function F() native` ⏎ `{doc for F}` | `F.doc_comment = None` | 0 |

  On the 29 F4SE `.psc` files on disk, `Location.psc` fails with 11 errors and
  `ObjectReference.psc` with 31 (the first is `bool Function IsSameLocation(...)` ⏎ `{Returns true
  if …}` at `Location.psc:39-40`).
- **Impact**: Bodied functions that follow the documented convention vanish from the AST, and their
  locals pollute script scope. `extender_preflight` counts the file as an input error but still runs
  `analyze_source_compatibility` on the damaged AST. Extender calls inside a dropped function are
  then missed, in exactly the documented SKSE/F4SE sources the tool exists to scan. Errors are
  reported, so this is not silent, and it is MEDIUM.
- **Related**: AUDIT_SCRIPTING_2026-08-12 noted that the `r5_round_trip` rumble fixture only passed
  because of trailing same-line `{doc}` comments. This is the general case.
- **Suggested Fix**: After each header's `expect_eol()`, accept an optional `DocComment` (with the
  newlines around it) and attach it to that item: script header, function, event, property, group,
  and struct member. Keep the leading-doc path only if a corpus shows it is used. Add the
  `Location.psc:39-41` shape as a test.

_Source: `AUDIT_PAPYRUS_2026-10-05.md` (PEX-D4-2026-10-05-02), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
