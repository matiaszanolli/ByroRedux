# #4321 SCR-D4-2026-09-14-02: the Pratt loop's newline-skipping `peek()` glues a line starting with `(` onto the previous statement's expression, giving a silent wrong AST with zero errors

**Labels**: medium,scripting,bug
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`

- **Severity**: MEDIUM (silent wrong AST from valid source; no runtime lowering consumes `.psc` ASTs today)
- **Dimension**: Papyrus Lexer & Pratt Parser
- **Untrusted-Input**: Partial (same reach as D4-01; also affects every in-repo test and tooling result built from hand-written `.psc`)
- **Location**: `crates/papyrus/src/parser/expr.rs::parse_expr_bp_inner` (`while let Some(tok) = self.peek()`); `crates/papyrus/src/parser/mod.rs::peek_with_span` (skips `Token::Newline`)
- **Status**: NEW. Same root cause as #2656 (closed), which was fixed only at `parse_property_flags`.
- **Description**: `docs/engine/papyrus-parser.md` makes `Token::Newline` the statement terminator unless `\` joins lines, and `preprocess` has already removed every `\` continuation. After an operand, though, the loop's `peek()` skips the newline. A next line starting with `(` becomes a call postfix on the previous expression, and the next statement's `.X()` suffix chains on. `(expr as Type).Method()` is valid statement syntax, so valid source reaches this.
- **Evidence**: The orchestrator confirmed `peek()` skips newlines while `peek_raw()` does not. Dim 4 probe, all with `errs=0`:
  1. `SetStage(10)` ⏎ `(akRef as ObjectReference).Disable()` → one `ExprStmt`: `Call{MemberAccess{Call{Call{SetStage,[10]},[akRef as ObjectReference]},Disable},[]}`.
  2. `If akActionRef == Game.GetPlayer()` ⏎ `(GetOwningQuest() as QF_Foo).Bar()` ⏎ `EndIf` → the condition absorbs the body line, and the body is empty.
  3. `Actor a = b as Actor` ⏎ `(a as ObjectReference).Disable()` → one `VarDecl`.

  Prevalence: 0 hits in the 33 on-disk `.psc` files. The Skyrim and Starfield `.psc` corpora are not on disk, so vanilla prevalence is **UNVERIFIED**.
- **Impact**: Statements silently disappear into a neighbouring expression, and the `If` condition/body split gets corrupted. `extender_preflight` under-reports calls. The psc-vs-pex fidelity gate cannot catch it, because the r5 fixtures lack the shape.
- **Related**: #2656, #1734
- **Suggested Fix**: Decide postfix/infix continuation with `peek_raw()` in `parse_expr_bp_inner`. Audit the other same-line `peek()`/`check()` decisions (`parse_type`'s `[`, `parse_qualified_ident`'s `:`, `parse_call`'s comma), and pin with a test built from example 1.

_Source: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`_

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives / spawn paths / walkers)
- [ ] **TESTS**: A regression test pins this specific fix
