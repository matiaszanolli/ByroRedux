# #5888: PEX-D4-2026-10-15-06: `Parent` is rejected in name positions, so F4SE `ObjectReference.psc`'s `struct ConnectPoint { string parent }` loses a member

- **Labels**: low,bug,scripting
- **Filed from**: `docs/audits/AUDIT_PAPYRUS_2026-10-15.md`

**Full title**: `Parent` is rejected in name positions, so F4SE `ObjectReference.psc`'s `struct ConnectPoint { string parent }` loses a member. It is listed as a KNOWN_GAP but no issue tracks it

- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/mod.rs:391-407` (`keyword_as_ident` has no `KwParent`). Reached from `parse_variable_body` (`crates/papyrus/src/parser/stmt.rs:251`) and `parse_member_access` (`crates/papyrus/src/parser/expr.rs:270`). It is listed in the test's `KNOWN_GAPS` at `crates/papyrus/src/parser/script.rs` (`fo4_script_sources_parse_clean_when_present`).
- **Status**: NEW (an untracked known gap: closed/open searches for "Parent keyword", "ObjectReference.psc" and "string parent" return nothing)
- **Untrusted-Input**: Yes
- **Description**: F4SE's shipped, compiled `ObjectReference.psc:1211-1222` declares `string parent` as a struct member, so the reference compiler accepts `parent` as a name. The parser records an error and drops the member, leaving 9 of 10 members. `cp.parent` member access would fail the same way.
- **Evidence**: Corpus probe: `ObjectReference.psc items=295 errs=1 first="expected identifier (variable name), found 'Parent'"`. The other 32 files are clean.
- **Impact**: The `ConnectPoint` struct has the wrong shape. An error is recorded, so this is not silent.
- **Suggested Fix**: Accept `KwParent` as an identifier in declaration-name positions and after `.`. Keep `Parent` as `Expr::ParentAccess` only in expression-primary position. Then remove the KNOWN_GAPS entry; the gate fails until it is removed.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
