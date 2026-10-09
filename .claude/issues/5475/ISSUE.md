# #5475: TD8-2026-10-08-02: `crates/plugin/examples/cond_dump.rs` is a committed `//! TEMP:` probe that the #5114 disposable-example guard does not recognise

**Labels**: low,tech-debt,bug,test-gap
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5475

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-08.md` — `TD8-2026-10-08-02` (HEAD `00f580e09`)

**Publish note**: Same guard-gap class as OPEN #5330 ("Scratch:" marker) but a different marker word and file; fixing #5330 alone does not catch `//! TEMP:`. Coordinate the `LINE_START_MARKERS` edit with #5330.

- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**:
  - `crates/plugin/examples/cond_dump.rs:1` ("`//! TEMP: dump one PACK's parsed CTDA conditions.`", 19 lines, 6 bare
    `unwrap()`s)
  - The guard: `byroredux/src/workspace_hygiene_tests.rs:55-58` (`LINE_START_MARKERS = ["throwaway", "one-off", "temp scratch"]`)
- **Status**: NEW. The same guard-gap class as OPEN #5330 (the "Scratch:" marker), with a different marker word.
- **Age**: `00f580e09` (HEAD, 2026-10-08), inside the "Eat and Sleep" feature commit.
- **Effort**: trivial
- **Description**:
  - #5114's commit message names "TEMP … not for commit" as one of the variants it was closing.
  - The guard only matches "temp scratch" at the start of a line, or "not for commit" anywhere. "TEMP:" passes.
  - The probe also carries two of the TD8-01 lints.
- **Related**: #5114 (CLOSED), #5330 (OPEN), TD8-2026-10-08-01.
- **Suggested Fix**:
  - Delete the probe. If the dump is worth keeping, fold it into a `cond` console subcommand or a documented example.
  - Add `"temp:"` and `"temp "` to `LINE_START_MARKERS` together with #5330's `"scratch"`.

## Completeness Checks
- [ ] **SIBLING**: Other committed examples scanned for `TEMP` / `temp:` module docs
- [ ] **TESTS**: A regression test pins this specific fix (the hygiene guard trips on a `//! TEMP:` doc line)
