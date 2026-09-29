# PEX-D4-2026-09-29-02: `expect()` / `expect_ident()` skip newlines, so a keyword alone on a line pulls its mandatory operand up from the next line with zero errors

**Labels**: low,bug,scripting

**Source report**: `docs/audits/AUDIT_PAPYRUS_2026-09-29.md`

- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/mod.rs:179-180` (`expect`) and `mod.rs:239-240`
  (`expect_ident`). Call sites include:
  - `script.rs:99-100` (ScriptName) and `:104` (Extends target)
  - `:194` (Import), `:287-288` (Function), `:319-320` (Event), `:398-399` (Property)
  - `:582-583` (`Auto` ⏎ `State`), `:661` (Struct), `:703` (Group)
  - `stmt.rs:245` (local keyword-typed declaration)
  - If/While condition start, through the Pratt prefix `advance()`.

  Also `docs/engine/papyrus-parser.md:243`.
- **Status**: NEW. #4763's commit message names this as a "broader class than this issue". No open
  or closed issue tracks it, and the skill lists it only as a known residual.
- **Untrusted-Input**: Yes
- **Description**: Since #4321, #4472 and #4763, every construct-*continuation* decision reads the raw
  token stream. But `expect()` and `expect_ident()` still call `skip_newlines()` before matching a
  construct's *mandatory* next token. So a keyword alone on a line still reaches across the newline.
- **Evidence**: Probe results. Every row returns errors=0:

  | Input | Parsed as |
  |---|---|
  | `Auto`⏎`State S` | `State{is_auto: true}` |
  | `ScriptName`⏎`Foo` | name `Foo` |
  | `ScriptName Foo Extends`⏎`Bar` | `parent = Some("Bar")` |
  | `Function`⏎`F()`, `Event`⏎`OnInit()`, `Int Property`⏎`P Auto`, `Import`⏎`Debug`, `Group`⏎`G`, `Struct`⏎`S` | items parsed normally |
  | local `Int`⏎`x = 5` | `VarDecl` |
  | `If`⏎`true`, `While`⏎`true` | conditions glued |

  At top level, `Int`⏎`x = 5` does raise 2 errors, which is inconsistent with the local path.
- **Impact**: The parser accepts source that the reference compiler rejects, because Papyrus treats
  EOL as significant. Unlike the #4472/#4763 class, the glued AST is the author's evident intent: a
  mandatory operand is pulled forward and nothing is attributed to the wrong construct. That is why
  this is LOW and not MEDIUM. The docs are also stale: `papyrus-parser.md:243` still says "#4472 tracks
  the remaining newline-skipping decision sites". Both #4472 and #4763 are closed, and this remaining
  class is not documented.
- **Related**: #4763, #4472, #4321 (same line-terminator contract)
- **Suggested Fix**: Add `expect_raw`/`expect_ident_raw` for operands that must share the line with their
  keyword: state, function, event, property, script, import, group and struct names, the `Extends`
  target, and the If/While condition start. Add one probe-style test per site, following the #4763 block
  in `script.rs`. Update the Pitfalls bullet.

**Validated at HEAD 9fcfdc3fc**: `Parser::expect` and `Parser::expect_ident` in `crates/papyrus/src/parser/mod.rs` both still begin with `self.skip_newlines()`; `docs/engine/papyrus-parser.md` still says "#4472 tracks the remaining newline-skipping decision sites".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
