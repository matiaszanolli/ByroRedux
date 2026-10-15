# #5890: PEX-D4-2026-10-15-08: doc and code-comment rot around the #5325 docs, logos 0.16 and a03cc727f

- **Labels**: low,documentation,doc-rot,scripting
- **Filed from**: `docs/audits/AUDIT_PAPYRUS_2026-10-15.md`

- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser (doc-rot)
- **Location**:
  - `docs/engine/papyrus-parser.md:69` and `:102` still say `ignore(ascii_case)`. The code has used `ignore(case)` since `15a6b1d2d`.
  - `docs/engine/papyrus-parser.md:110-111` and `crates/papyrus/src/parser/script.rs:321-322` say "a leading `{…}` is still accepted as a fallback for functions, events and properties".
  - `crates/papyrus/src/parser/stmt.rs:77-79` says it "skips leading newlines / doc-comments".
  - `crates/papyrus/src/parser/mod.rs:279-280` calls `;;` a doc comment.
  - `crates/papyrus/src/token.rs:347-348` has an empty `impl Token {}`, left by a03cc727f.
- **Status**: NEW
- **Untrusted-Input**: No
- **Description / Evidence** (probe):
  - The leading-doc fallback never attaches anywhere. At top level the item loop discards `skip_newlines_collect_doc()`'s result (`script.rs:77`), so `{lead}` ⏎ `Function F()` gives F with `doc=None` and 0 errors. Inside State, Group or property-accessor containers the catch-all arm sends the `DocComment` to `parse_type`, which is a recorded parse error (`leading doc fn in state`: 1 error).
  - `parse_stmt_inner` calls `skip_newlines` only.
  - Doc comments are `{…}`, not `;;`.
  - No test pins the leading form.
- **Impact**: Readers are misled about accepted input; no runtime effect.
- **Suggested Fix**: Correct the three doc statements, or make the fallback real by threading the top-level loop's collected doc into the item parsers. Delete the empty `impl`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
