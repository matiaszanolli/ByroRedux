# #5168 — TOOL-D2-2026-10-02-01: The `loadscreen.census` output line has a 14-space whitespace run in the middle (a lost line continuation)

Labels: low,tech-debt,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/5168

From `docs/audits/AUDIT_TOOLING_2026-10-02.md` (HEAD `e737f06bf`).

- **Severity**: LOW
- **Dimension**: Protocol & Registry
- **Exposure**: console and byro-dbg users; the p6 smoke greps this line
- **Location**: `byroredux/src/commands/world_info.rs:89-140` (`e60911864`); the same commit at `:146` (`EntitiesCommand::description`)
- **Status**: NEW
- **Description**: The `format!` string reads `"…locations={},              malformed={}…"`. A multi-line literal lost its trailing `\`, so the indentation became part of the output. The same commit also left `"Show entity count and component breakdown"    }` on one line. Neither change went through rustfmt.
- **Evidence**: `git show e60911864 -- byroredux/src/commands/world_info.rs`
- **Impact**: The console output is cosmetically broken. `docs/smoke-tests/p6-loading-model.sh:126` greps only `eligible=[1-9]`, so the smoke is unaffected today.
- **Related**: none
- **Suggested Fix**: Restore the `\` continuation and run `cargo fmt` on the file.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other writers / frontends / request variants named above)
- [ ] **TESTS**: A regression test pins this specific fix

