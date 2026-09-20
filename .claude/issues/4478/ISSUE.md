# PEX-D4-2026-09-19-02: papyrus-parser.md depth-cap/newline sections predate #4320/#4321; stale paren-depth comment

- **ID**: D4-02
- **Labels**: low,scripting,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_PAPYRUS_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4478

**Severity**: LOW (doc rot) · **Dimension**: Papyrus Lexer & Pratt Parser · **Untrusted-Input**: No
**Source**: `docs/audits/AUDIT_PAPYRUS_2026-09-19.md` (PEX-D4-2026-09-19-02) · **Location**: `docs/engine/papyrus-parser.md:223-229` (depth caps), `:108-110` + `:234-236` (newlines), `:267-279` (test counts); `crates/papyrus/src/parser/expr.rs:944-949` (inline comment)

**Description**
The parser contract doc predates both #4320 and #4321:
- The depth-cap section still says the cap works by "`parse_expr_bp` increments/decrements `Parser::expr_depth` around each recursion" — it omits #4320's `enter_chain_link` charging of iteratively built postfix/binary chains, the load-bearing half of the cap since the fix.
- The "Newlines are significant" section and Pitfalls name only empty-`Return` as the `peek_raw` load-bearer; the rule "a newline terminates an expression; an operator ending a line still continues" (#4321) is undocumented.
- Test counts say "73 total (expr.rs — 35)"; the crate is now 100 unit + 4 round-trip with 7 new expr.rs guards.
- Separately, the inline comment in `depth_cap_accepts_legitimate_nesting` (`expr.rs:944-949`) claims "Each paren-pair contributes 2 to expr_depth" — measured 1/pair (200 pairs parse; the cap admits ≈255 pairs), so the test's margin rationale is factually wrong even though the test passes.

**Evidence**
Doc text at the cited lines vs `expr.rs:41-70` (entry-charge + per-chain-link charge + value-restore); dim-4 probe paren rows.

**Impact**
A future editor loosening the cap or touching the loop reads a wrong model of the depth ledger and of the newline contract — exactly the drift the contract doc exists to prevent.

**Related**: #4320, #4321

**Suggested Fix**
Update the depth-cap section to describe entry-charge + per-chain-link charge + restore-on-every-exit; add a "newline ends an expression" bullet to Pitfalls; refresh test counts; correct the paren-pair comment to 1 unit/pair.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only), but confirm the corrected paren comment matches a measured boundary
