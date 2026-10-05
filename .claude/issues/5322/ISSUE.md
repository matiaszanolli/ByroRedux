# #5322 — PEX-D4-2026-10-05-01: expect_eol / parse_expr never enforce end-of-statement or end-of-input, so the FO4 `is` operator silently becomes `If f` plus a VarDecl named Actor

- **Labels**: medium,scripting,game:fo4,bug
- **Filed from**: `docs/audits/AUDIT_PAPYRUS_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5322

- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/mod.rs:241-262` (`expect_eol`, every non-`Newline` arm
  returns `Ok(())`, commented "be lenient — many Papyrus scripts don't have strict EOL") ·
  `crates/papyrus/src/lib.rs:18-43` (`parse_expr` returns `Ok` without checking that input is
  exhausted) · `crates/papyrus/src/token.rs` (no `Is` token; only `As` at `:213`)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: The statement terminator is not enforced. When the Pratt loop stops at a token
  it does not recognise, the statement ends there without an error. The rest of the line is then
  parsed as a new statement. `parse_expr` likewise returns its longest prefix and ignores whatever
  follows. Valid source normally hides this. FO4's `is` type-check operator exposes it: `is` has no
  token and no AST node, so it lexes as an identifier and starts a second "statement". (On the `.pex`
  side, #4477 lowers it to a `Cast`.)
- **Evidence**: These are probe results from a scratch crate with a path dependency on
  `byroredux-papyrus`:

  | Input | Result | errors |
  |---|---|---|
  | `If f is Actor` ⏎ `EndIf` (inside a function) | `If { condition: f, body: [VarDecl { ty: Object("is"), name: "Actor" }] }` | **0** |
  | `parse_expr("x is Actor")` | `Ok(x)`, span 0..1 | n/a |
  | `parse_expr("a > 5 b")` | `Ok(a > 5)` | n/a |
  | `parse_expr("Game.GetPlayer() garbage tokens")` | `Ok(Game.GetPlayer())` | n/a |

  `If f is Actor` is documented FO4 syntax (falloutck wiki, Operator Reference).
- **Impact**: This is a silent wrong AST. The condition changes from a type check to a
  truthiness check, and a fabricated declaration is injected, with zero errors, so a strict-fail
  caller that checks `result.1.is_empty()` accepts it. A recognizer could match the wrong condition.
  The same root cause makes the debug console evaluate a prefix of a mistyped expression without
  complaint. It is reachable through the same two paths as the #4321/#4472/#4763 class: the offline
  `extender_preflight` `scan_source` and the debug-console `parse_expr`. That reachability, and
  precedent (#4321, #4472 and #4763 were all MEDIUM), are why this is MEDIUM rather than HIGH.
- **Related**: #4477 (the `.pex` side of `is`), #4321, #4472, #4763 (silent-glue class), finding 03
- **Suggested Fix**: Make `expect_eol` report an error when the next raw token is neither `Newline`,
  EOF, nor a `DocComment`. Then recover to the next line. Make `parse_expr` reject tokens left after
  the expression. Separately, lex `Is` and add an AST form for it (or the same `Cast` the `.pex` side
  uses), so the two frontends agree.

_Source: `AUDIT_PAPYRUS_2026-10-05.md` (PEX-D4-2026-10-05-01), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
