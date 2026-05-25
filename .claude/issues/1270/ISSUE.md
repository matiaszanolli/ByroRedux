# #1270 — SAFE-DIM3-NEW-02: Papyrus expression parser has no recursion-depth guard

URL: https://github.com/matiaszanolli/ByroRedux/issues/1270
Filed: 2026-05-25
Labels: low, safety, bug

> Snapshot of the issue as filed. GitHub is authoritative for current state.

---

## Source

`/audit-safety 3` sweep — `docs/audits/AUDIT_SAFETY_2026-05-25_DIM3.md` (Dimension 3: Memory Safety, §3.7 Stack overflow risk).

## Location

`crates/papyrus/src/parser/expr.rs:22-61` — `parse_expr_bp`

## Description

The Pratt expression parser recurses through the RHS of every binary operator and through every parenthesised sub-expression with no depth cap. A source file containing `((((((... ))))))` to arbitrary depth would stack-overflow the parser rather than emit a `ParseError`.

## Evidence

- `parse_expr_bp` self-call at expr.rs:61 (RHS of binary op).
- `parse_prefix` → `parse_expr` → `parse_expr_bp` chain at expr.rs:137 (parenthesised expression).
- `parse_prefix` → `parse_expr_bp` for prefix unary at expr.rs:145 / 157.
- No `MAX_EXPR_DEPTH` constant anywhere in the parser.

## Impact

A modder-authored `.psc` (or a fuzz-generated one) can crash the parser. Papyrus source input is treated as trusted today, but the engine surface includes "compile mod scripts at load time" (M30 Phase 2), so the input surface is partially user-controlled. A worst-case crafted source file aborts the engine on script load.

## Related

- Papyrus parser is the entry point for the M30 Phase 2 bytecode compile path.
- Pratt parser design is otherwise sound — only the depth bound is missing.

## Suggested Fix

Carry `&mut depth: u32` through `parse_expr_bp` and `parse_prefix`; emit `ParseError::ExpressionTooDeep` at, say, depth 256. Increment on every recursive call, decrement on return (or use a guard type). Add a `MAX_EXPR_DEPTH` constant in `parser/mod.rs` so the threshold is discoverable.

## Completeness Checks

- [ ] **UNSAFE**: N/A.
- [ ] **SIBLING**: Cross-check `crates/papyrus/src/parser/` for any other recursive descent paths — statement parser, type parser. If any recurse through user input, same guard pattern.
- [ ] **DROP**: N/A.
- [ ] **LOCK_ORDER**: N/A.
- [ ] **FFI**: N/A.
- [ ] **TESTS**: Add a regression test that feeds a depth-512 parenthesised expression to `Parser::parse_expr` and asserts it returns `ParseError::ExpressionTooDeep` (not a stack overflow). A second test should confirm a deeply nested but legitimate expression at depth 200 still parses successfully.
