# #4320 SCR-D4-2026-09-14-01: left-deep postfix/infix chains bypass `MAX_EXPR_DEPTH`; ~0.4 MB of `.psc` or console text aborts with a stack overflow on AST Drop

**Labels**: medium,scripting,safety,bug
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`

- **Severity**: MEDIUM (the domain table says HIGH for parser stack overflow; downgraded one step because no runtime path feeds game or mod `.psc` into the parser. Re-escalate if a `.psc` ingest path lands.)
- **Dimension**: Papyrus Lexer & Pratt Parser
- **Untrusted-Input**: Partial (`extender_preflight` on mod-author `.psc`; localhost debug console `eval_expr` → `parse_expr`, 16 MB frame cap)
- **Location**: `crates/papyrus/src/parser/expr.rs::parse_expr_bp_inner` (postfix `continue` arms for `.` / `[` / `(` / `as`, and the infix loop); `crates/papyrus/src/ast.rs` (`Box`ed `Expr` children, derived recursive Drop/Clone/Debug)
- **Status**: NEW (no match for postfix-chain / Drop overflow; #1270, #3783 and #3933 cover recursion, not iterative chain depth)
- **Description**: `MAX_EXPR_DEPTH` counts only recursive `parse_expr_bp` calls. `a.a.a…`, `a()()…`, `a[0][0]…` and `a+a+…` are built by the *iterative* loop, which wraps `lhs` in a new `Box` each iteration while `expr_depth` stays at 1. The finished tree is as deep as the input is long, and Drop plus every recursive walker recurse once per level. The cap's own doc, and the 09-11 matrix's "Total: Yes", assume AST depth ≤ 256.
- **Evidence**: The orchestrator confirmed the postfix arms never touch `expr_depth`. Dim 4 probe (8 MB main thread): `chain_member` at 100k–170k terms parses and drops fine; at 200k / 400k / 700k / 1M terms it hits `thread 'main' has overflowed its stack … aborting` (SIGABRT). `chain_add`, `chain_call` and `chain_index` behave the same, as does `parse_expr` on 1M terms (158 ms parse, then abort on drop).
- **Impact**: Process abort of the engine via the debug console, or of tooling that scans third-party `.psc`. Any recursive AST consumer inherits the unbounded depth; the #4113 reasoning is `.pex`-only.
- **Related**: #1270, #3783, #4113
- **Suggested Fix**: Count chain length as depth: increment (or locally count against `MAX_EXPR_DEPTH`) on each postfix/infix iteration that wraps `lhs`, returning `ExpressionTooDeep`. Regression test: `a` + `.a` ×10,000 via `parse_expr` returns `ExpressionTooDeep`.

_Source: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`_

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives / spawn paths / walkers)
- [ ] **TESTS**: A regression test pins this specific fix
