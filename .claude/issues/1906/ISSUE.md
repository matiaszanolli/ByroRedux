# #1906 — SCR-D4-NEW-01: Papyrus int/float literal regexes swallow a leading minus, silently dropping adjacent subtraction

_Filed from `docs/audits/AUDIT_SCRIPTING_2026-07-06.md`. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 1906 --json state`)._

---

**Severity**: MEDIUM · **Dimension**: Papyrus Lexer & Pratt Parser · **Untrusted-Input**: Yes
**Location**: `crates/papyrus/src/token.rs:228,231-232`
**Source**: audit `docs/audits/AUDIT_SCRIPTING_2026-07-06.md` (SCR-D4-NEW-01)

## Description
`IntLit`/`FloatLit` regexes carry an optional leading minus (`-?[0-9]+`, priority 2/3 — above the `Ident` regex's priority 1). Under logos longest-match, a `-` immediately followed by a digit (no intervening space) is eaten into the literal as a negative sign, even when it is binary subtraction. The Pratt loop only treats `Token::Minus` as an infix operator, so it then sees two adjacent value tokens, breaks, and the second operand is dropped with no diagnostic.

## Evidence
Live probes: `lex("a-10")` → `[Ident("a"), IntLit(-10)]`; `parse_expr("5-3")` → `Ok(IntLit(5))` — the `-3` silently vanishes. Only whitespace around the `-` (`a - 10`) parses as subtraction. Common no-space idioms (`arr[len-1]`, `x = a-1`) mis-parse. Two existing tests (`test_negative_int_literal`, `test_lex_int_literals`) currently lock in the behavior.

## Impact
Wrong AST — a silent, non-crashing mis-parse of any adjacent subtraction. **Latent** in the live engine: the production attach path decompiles `.pex` straight to the AST and never touches this lexer; `parse_script`/`parse_expr` callers today are test-only over curated scripts. It becomes live the moment a `.psc` frontend feeds real source (the Obscript/SCTX Phase-5 work or any direct `.psc` ingest).

## Related
Divergence from the reference Papyrus compiler, where `a-10` is subtraction. Not HIGH — it terminates, no crash/DoS.

## Suggested Fix
Remove `-?` from both literal regexes and rely on the already-present unary-minus prefix path (`Token::Minus` → `Expr::Neg`). Update the two tests that assert the merged sign.

## Completeness Checks
- [ ] **SIBLING**: All three literal regexes (int, and both float alternations at `:231-232`) fixed together, not just `IntLit`
- [ ] **TESTS**: A regression test pins `a-10` / `5-3` parsing as subtraction; `test_negative_int_literal` / `test_lex_int_literals` updated to expect unary-minus-of-literal
