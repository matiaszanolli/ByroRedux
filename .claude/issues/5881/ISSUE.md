# #5881: PEX-D4-2026-10-15-01: `preprocess` applies `\` line continuation inside `;` comments, so the CK's own example silently drops the next line (and the CK-allowed `\ 

- **Labels**: medium,bug,scripting
- **Filed from**: `docs/audits/AUDIT_PAPYRUS_2026-10-15.md`

**Full title**: `preprocess` applies `\` line continuation inside `;` comments, so the CK's own example silently drops the next line (and the CK-allowed `\ ; comment` is rejected)

- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/lexer.rs:7-36` (`preprocess` elides every `\` immediately followed by `\n`/`\r\n`/`\r`, before lexing, without knowing about comments or strings)
- **Status**: NEW
- **Untrusted-Input**: Yes (`.psc` files, `script.exec` console text, offline preflight)
- **Description**: The FO4 and Skyrim CK *Script File Structure* pages say: "Single line comments … consume all text after the semicolon, including any backslash characters that would otherwise let you continue onto the following line". They also say a comment may follow the continuation slash. Their example is `; Slash in this line is ignored, because it is in a comment \` ⏎ `x = 1 + 2 ; This line isn't inside the above comment`. `preprocess` runs before the lexer and joins the two lines, so the next line becomes part of the comment and disappears. The opposite case also breaks: `\` followed by `; note` or by trailing blanks is not elided. It lexes as `Token::Backslash`, which is a parse error, and the statement is lost.
- **Evidence** (probe):
  - The CK example inside `Function F()` gives F 0 statements and **0 errors**.
  - `Int x = 1 ; see Data\Scripts\` ⏎ `Int y = 2` keeps only `x`, with **0 errors**.
  - Top level `; dir C:\` ⏎ `Int Property P Auto` loses property P, with **0 errors**.
  - `x = 1 + \ ; note` ⏎ `2` (CK-legal) gives 3 errors and F is dropped. `x = 1 + \   ` ⏎ `2` gives 3 errors and F is dropped.
- **Impact**: This is a silent wrong AST with content loss, the same class as #5019 (MEDIUM): any statement, property or function header after a comment ending in `\` vanishes with zero diagnostics. A `.psc` frontend that drops lines also skews the offline extender preflight, whose missed calls go unreported.
- **Related**: #5019, #4479; finding 02 has the same root.
- **Suggested Fix**: Make continuation a lexer rule instead of a pre-pass. Add a logos skip rule such as `\\[ \t]*(;[^\r\n]*)?(\r\n|\r|\n)`. The `;` comment rule then naturally consumes a `\` inside a comment, spans stay in original coordinates and `OffsetMap` can go, which also fixes 02. Pin the CK example verbatim.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
