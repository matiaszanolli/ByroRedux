# PEX-D4-2026-09-19-03: lexer silently mislexes 0x, 1e5, and CR-only line endings with zero diagnostics

- **ID**: D4-03
- **Labels**: low,scripting,bug
- **Filed from**: docs/audits/AUDIT_PAPYRUS_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4479

**Severity**: LOW · **Dimension**: Papyrus Lexer & Pratt Parser · **Untrusted-Input**: Yes
**Source**: `docs/audits/AUDIT_PAPYRUS_2026-09-19.md` (PEX-D4-2026-09-19-03) · **Location**: `crates/papyrus/src/token.rs:241-247` (literal regexes), `:105` (`skip r"[ \t\r]+"`), `:108` (`Newline` = `\n` only)

**Description**
Three lexer-level shapes produce no diagnostic while yielding token streams a reader would not expect:
(a) `0x` with no hex digits is not matched by the hex regex, so it lexes as `IntLit(0)` + `Ident("x")` — invalid source silently becomes plausible valid tokens;
(b) `1e5` lexes as `IntLit(1)` + `Ident("e5")` (Papyrus has no exponent notation; 0 errors);
(c) `\r` is in the skip-whitespace regex and `Newline` matches only `\n`, so a CR-only (classic-Mac) file produces **no `Newline` tokens at all** — the entire file is one line to the parser, the newline-terminator contract is void, and the #4321 protection is vacuous on such files (probe: `SetStage(10)`␍`(akRef).Disable()` → 1 glued statement, 0 errors). CRLF files are handled correctly (`\r` skipped, `\n` kept). Literal *overflow* is fine (`0xFFFFFFFFFFFFFFFF` errors cleanly, no panic).

**Evidence**
Dim-4 probe rows `0x-alone`, `1e5`, `pure-CR endings` (AUDIT_PAPYRUS_2026-09-19).

**Impact**
Exotic/typo'd source gets silently mis-tokenized instead of a precise lex error; the CR-only case neutralizes the entire statement-terminator design on such files. No known corpus hits — hence LOW.

**Related**: #1908 (same "lex must not lie" theme)

**Suggested Fix**
(a) make a `0[xX]` prefix with no digits a lex error (or fold it into one IntLit + error); (b) reject `e`-adjacent mislexes with an "exponents not supported" lex error; (c) either treat a lone `\r` as `Newline` (matching many legacy tools) or lex-error on a `\r` not followed by `\n`.

## Completeness Checks
- [ ] **SIBLING**: Check the other literal regexes (float, char) for the same prefix-only shape
- [ ] **TESTS**: One regression test per shape asserting a lex error (or the chosen `\r` behavior)
