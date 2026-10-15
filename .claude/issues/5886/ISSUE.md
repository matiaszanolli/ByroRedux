# #5886: PEX-D4-2026-10-15-04: a same-line doc comment lets the next item or statement share the line, with 0 errors (`take_trailing_doc` never requires the line break after the doc)

- **Labels**: low,bug,scripting
- **Filed from**: `docs/audits/AUDIT_PAPYRUS_2026-10-15.md`

- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/mod.rs:170-180` (`take_trailing_doc` consumes the following `Newline` only "if present") and `:292` (`expect_eol` returns `Ok` on a `DocComment` without consuming it, so the terminator is a doc rather than a line break)
- **Status**: NEW. The function-body form is new in e96d687ec; the top-level form predates it.
- **Untrusted-Input**: Yes
- **Description**: #5322 made `expect_eol` enforce end-of-statement, but a `{…}` still counts as one. Before e96d687ec, a doc left by `expect_eol` hit `parse_block` and errored. Now `take_trailing_doc` swallows it and whatever follows on the same line parses as the next body statement or item.
- **Evidence** (probe, all **0 errors**):
  - `Function F() {d} Int x = 1` gives F with 1 body statement.
  - `Function F() native {d} Int Property P Auto` gives F and P.
  - `Int a {doc} Int b` gives two variables (pre-existing).
  - The intended forms parse as specified: a trailing doc on the next line, or a doc on the same line followed by a line break.
- **Impact**: The parser accepts malformed source; the CK grammar puts the doc on its own line. No crash, no loop.
- **Suggested Fix**: After consuming the doc in `take_trailing_doc`, require `Newline`/EOF by calling the `expect_eol` error path when anything else follows. Have `expect_eol`'s `DocComment` arm do the same (consume the doc, then require a line break).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
