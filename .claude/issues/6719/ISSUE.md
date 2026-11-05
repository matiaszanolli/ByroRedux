# RF9-2026-11-05-01: Production comments carry issue history that `git log` already holds

Labels: low,documentation,tech-debt

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, side notes
**Status**: NEW. Part of #6680

## Location
- `byroredux/src` (80,248 code lines, 36,322 comment lines, 3,778 citing an issue)
- `crates/renderer` (42,058 / 28,879 / 2,958)
- `crates/nif` (43,647 / 21,231 / 2,423)
- `crates/plugin` (40,107 / 18,201 / 1,691)
- `crates/scripting` (55,928 / 15,587 / 972)
- `crates/core` (13,433 / 12,349 / 798)

## Why
Across production code, excluding `#[cfg(test)]` modules and test-named files, there are 333,804 code lines and 150,932 comment lines, of which 14,077 cite an issue number. Much of this narrates how the code got here rather than the rule it follows today. `record_skinned_blas_refit` is the clearest case: 971 lines, of which 382 are code.

## Change
Add a short rule to `docs/contributing.md`: a comment states the current rule, plus the issue number if useful. Apply it as code is touched, and do not run a mass rewrite. #6708 is a natural first target.

## Constraints
Some guards pin comment text, such as `get_distance_variant_keeps_its_doc_comment` and `module_doc_covers_the_buoyancy_phase_and_its_ordering`, so check for them before editing. The triage issues #6686 to #6689 classify these guards.

## Acceptance Criteria
- `docs/contributing.md` states the rule
- A first pass over one named file changes only comment lines, confirmed from its diff
- All tests that read that file still pass

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6719 --json state`)._
