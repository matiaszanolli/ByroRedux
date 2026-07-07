# #1908 — SCR-D4-NEW-02: Out-of-range Papyrus integer/float literals silently become 0

_Filed from `docs/audits/AUDIT_SCRIPTING_2026-07-06.md`. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 1908 --json state`)._

---

**Severity**: LOW · **Dimension**: Papyrus Lexer & Pratt Parser · **Untrusted-Input**: Yes
**Location**: `crates/papyrus/src/token.rs:79-93` (`parse_int`/`parse_float` → `unwrap_or(0)`)
**Source**: audit `docs/audits/AUDIT_SCRIPTING_2026-07-06.md` (SCR-D4-NEW-02)

## Description
A lexable-but-out-of-range literal (`0xFFFFFFFFFFFFFFFF`, a huge decimal) overflows `i64`/`f64` and, via `unwrap_or(0)` (`:85,87`) / `unwrap_or(0.0)` (`:92`), silently becomes `IntLit(0)`/`FloatLit(0.0)` with no lex error.

## Evidence
`parse_int` uses `i64::from_str_radix(hex, 16).unwrap_or(0)` and `slice.parse().unwrap_or(0)`; `parse_float` uses `.parse().unwrap_or(0.0)`. No diagnostic is surfaced — this is silent wrong-value, not a crash (the no-panic checklist item passes).

## Impact
Cosmetic/rare; latent (same test-only `.psc` exposure as SCR-D4-NEW-01 — the live attach path is `.pex`→AST). A malformed literal reads as `0` rather than erroring.

## Related
Same lexer, same latency as SCR-D4-NEW-01. Both should be addressed before a real `.psc`/SCTX frontend goes live.

## Suggested Fix
On parse overflow, emit a lex error (or a saturating value with a recorded diagnostic) rather than `unwrap_or(0)`.

## Completeness Checks
- [ ] **SIBLING**: Both `parse_int` (hex + decimal arms) and `parse_float` handled the same way
- [ ] **TESTS**: A regression test pins that an out-of-range literal surfaces a diagnostic rather than silently becoming `0`
