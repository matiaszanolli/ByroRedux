# SCR-D4-01: No recursion-depth guard on nested statements — stack overflow from untrusted .psc

Filed as: matiaszanolli/ByroRedux#1712
Source audit: `docs/audits/AUDIT_SCRIPTING_2026-06-23.md`

- **Severity**: HIGH
- **Dimension**: Papyrus Lexer & Pratt Parser · Untrusted-Input: Yes
- **Location**: `crates/papyrus/src/parser/stmt.rs:88-136` (`parse_if_stmt`/`parse_while_stmt` → `parse_block` → `parse_stmt`)
- **Labels**: high, legacy-compat, bug

## Description
`MAX_EXPR_DEPTH=256` (#1270) guards expression recursion only. Block-statement recursion (`If`/`While` bodies) has no depth guard. A `.psc` with a few thousand nested `If`/`While` overflows the parser stack (abort, uncatchable) — the same class #1270 closed, on the statement axis.

## Impact
A hostile/corrupt `.psc` aborts the process. Untrusted-byte parser.

## Suggested Fix
Add a `stmt_depth` counter mirroring `expr_depth`, capped, returning a `StatementTooDeep` error.
