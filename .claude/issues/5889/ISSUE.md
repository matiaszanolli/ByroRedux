# #5889: PEX-D4-2026-10-15-07: an unterminated `;/` block comment or string literal is accepted with 0 errors

- **Labels**: low,bug,scripting
- **Filed from**: `docs/audits/AUDIT_PAPYRUS_2026-10-15.md`

**Full title**: an unterminated `;/` block comment or string literal is accepted with 0 errors; the block comment silently swallows the rest of the file

- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/token.rs:19-28` (`skip_block_comment` consumes to EOF and returns `Skip`), `:69-78` (`parse_string_literal` ends at the line break and returns the partial string), `:38-42` (unterminated doc comment, same pattern)
- **Status**: NEW (closed/open searches for "unterminated" find only #2185, the container hang)
- **Untrusted-Input**: Yes
- **Description**: Each callback recovers silently instead of routing to the `LexError` path that `parse_int` uses.
- **Evidence** (probe):
  - `…EndFunction` ⏎ `;/ oops` ⏎ `Function G()…` ⏎ `Function H()…` parses only F, with **0 errors**; G and H are gone.
  - `String s = "abc` ⏎ gives a `StringLit("abc")` with **0 errors**.
  - An unterminated `{` does surface an error, but only indirectly, through the enclosing body.
- **Impact**: A truncated or hand-edited file loses everything after the comment with no diagnostic. The input is invalid either way, so this is LOW, as with #4479, rather than #5019's MEDIUM.
- **Suggested Fix**: Make the three callbacks return an error on a missing terminator (logos `FilterResult::Error` / `Result`), so `lex()` records a `LexError` and inserts its placeholder.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
