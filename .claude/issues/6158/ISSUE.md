# TD2-2026-10-19-13: `DELIVERED_EVENTS` (18) and `DELIVERED_ALIAS_EVENTS` (16) share 14 hand-copied literals

Labels: low,scripting,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src/fragment/reference.rs:36-57`, `fragment/alias_scripts.rs:33-50`
- **Age**: 2026-10-13 (alias list), the four trap/destruction events were added to the reference list only (2026-10-18/19)
- **Effort**: trivial
- **Description**: the alias list is the reference list minus `OnDestructionStageChanged`/`OnTrapHit`/`OnTrapHitStart`/`OnTrapHitStop`, plus `OnAliasInit`/`OnAliasShutdown`. A new reference-side event must be added to two arrays and nothing records whether the alias omission is a decision (`raise_reference_event` only reaches `PapyrusReferenceScripts`).
- **Proposed fix**: one `const SHARED_REFERENCE_EVENTS: [&str; 14]` in `reference.rs`; build each public list from it (concat in a `const fn`, or a `LazyLock<Vec<&'static str>>`), with a comment on why the four are reference-only.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
