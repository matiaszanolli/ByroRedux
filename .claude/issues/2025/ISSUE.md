# SCR-D4-NEW2-01: A single out-of-range literal anywhere in a .psc file hard-fails the entire parse_script

**Labels**: medium, bug

**Severity**: MEDIUM
**Dimension**: Papyrus Lexer & Pratt Parser
**Untrusted-Input**: Yes (latent today — see Impact)
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-07-16.md`

## Location
`crates/papyrus/src/lib.rs:20-30` (`parse_expr`), `:62-72` (`parse_script`) — `if !lex_errors.is_empty() { return Err(...) }`

## Description
`parse_script`'s own doc comment promises a script parses partially on recoverable errors. That holds for parser-level errors (a malformed function is dropped, siblings still parse) but not lex-level errors: `lex_errors` is collected across the whole preprocessed source, and if non-empty, `parse_script` returns `Err` immediately — before the tolerant per-item-recovering parser ever runs. Pre-#1908, an out-of-range literal never produced a lex error (silent `0`), so this whole-file gate rarely tripped. Post-#1908 (which correctly turned that silent-`0` into a real lex error, fixing a prior gap), the same literal now always trips the whole-file gate — a single bad literal in one function of an otherwise-valid multi-hundred-line script now yields zero AST for the entire file, not just the offending item.

Verified current: `parse_script` (`crates/papyrus/src/lib.rs`) still returns `Err` immediately whenever `lex_errors` is non-empty, before `parser::Parser::parse_script()`'s own per-item recovery has any chance to run.

## Evidence
Live-verified via a temporary probe test (reverted, `git diff` clean): a 2-function script with one out-of-range literal in `Function A` returns `Err` with `Function B` never parsed at all, despite being valid and unrelated.

## Impact
Latent today — the live cell-loader attach path decompiles `.pex` directly and never calls `parse_script`/`parse_expr`; today's callers are curated test fixtures. But it undermines this dimension's stated resilience contract and will be live the moment a real `.psc` or Obscript/SCTX frontend feeds this parser — a strictly worse failure mode for modded-content ingest than either the pre-fix silent-`0` bug or the intended per-item-recoverable model.

## Related
Direct side effect of the #1908 fix (`token.rs` `parse_int`/`parse_float` → `Result`); not a regression of that fix itself (it remains correctly fixed) — but a gap in how far that fix threaded through the pipeline.

## Suggested Fix
Route lex errors through the same per-item recovery path as parse errors — either convert each `LexError` into a synthetic placeholder token so `skip_to_next_line` naturally isolates the damage, or scope lex-failure to the containing line so `parse_script` drops only the offending item. Add a regression test asserting a multi-function script with one bad literal still returns `Ok` with the unaffected functions present.

## Completeness Checks
- [ ] **TESTS**: A regression test asserting a multi-function script with one bad literal still returns `Ok` with unaffected functions present
