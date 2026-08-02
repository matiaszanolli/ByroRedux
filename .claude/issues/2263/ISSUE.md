# TD5-001: XXXX-protocol false-positive exclusion list doesn't yet name the two newest legitimate reference sites

Severity: low
Source audit: docs/audits/AUDIT_TECH_DEBT_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2263

**Dimension**: 5 (Stale Markers)
**Location**: `.claude/commands/audit-tech-debt/SKILL.md` — the Dimension-5 false-positive exclusion note (currently scoped to `reader.rs` and `records/misc/magic.rs` only)
**Status**: NEW

**Description**: The false-positive exclusion for the ESM `XXXX` extended-size sub-record tag is scoped by filename to `reader.rs` and `magic.rs` only. Commit `560c6741d` (2026-07-26, closes #1849) added two more legitimate references to the same `XXXX` protocol tag in `crates/plugin/src/esm/cell/wrld.rs:175` and `crates/plugin/src/esm/cell/mod.rs:871`. Both are the same false-positive class, but the exclusion rule's file list is now stale, so every future audit has to re-derive "is this a new marker or the same protocol tag in a new file" from scratch instead of checking a maintained list.

**Evidence**: `crates/plugin/src/esm/cell/wrld.rs:175` — `// through the XXXX extended-size escape, which`; `crates/plugin/src/esm/cell/mod.rs:871` — `/// \`XXXX\` extended-size escape — see \`EsmReader::read_sub_records\`.`. Issue #1849 confirmed CLOSED.

**Impact**: Process/documentation hygiene on the audit tooling itself; zero impact on shipped code, but wastes future-audit effort re-deriving something already known.

**Suggested Fix**: Extend the exclusion bullet to read "...in `reader.rs`, `records/misc/magic.rs`, `esm/cell/wrld.rs`, and `esm/cell/mod.rs`" — or better, key the exclusion on string content ("any comment referencing the ESM `XXXX` extended-size escape") rather than enumerating file paths, so it doesn't need updating again next time a new consumer of the OFST/XXXX path gets documented.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix, if applicable
