# TD8-2026-10-19-01: Five dead public items from the 10-15 list were never deleted (no open issue; #5745 closed without them)

Labels: low,scripting,esm-plugin,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW — **Dimension**: 8 — **Status**: still-live (prior TD8-2026-10-15-04, residue)
- **Location**: `crates/plugin/src/esm/records/misc/water.rs:1498` (`watr_to_params`, `record.params`; re-export `misc.rs:106`); `crates/scripting/src/dialogue.rs:153` (`QuestGates::conditions_of`, superseded by `shared_conditions_of` `:160`); `crates/scripting/src/fragment/alias_scripts.rs:254` (`started_quests`); `crates/scripting/src/fragment/populate.rs:506` (`populate_scene_fragments_from_pex`, only other mention is the `lib.rs:154` re-export); `crates/scripting/src/fragment/package.rs:36` (`PackageFragmentSet`, only other mention `lib.rs:173` re-export)
- **Age**: `watr_to_params` fa309d8d6 (2026-05-12); `conditions_of` 6709eb65e (10-12); `started_quests` caa81f212 (10-13)
- **Effort**: trivial
- **Description**: `git grep -nw <name> -- '*.rs'` returns only the definition (and the `pub use` line for the three re-exported ones). `watr_to_params` is a one-line field read ("adapter ... just returns the structured view"). The superseded-by-`shared_*`/`_detailed` pattern is the same one that left the first batch behind.
- **Evidence**: counts above re-run today; scripts in `/tmp/audit/tech-debt/d8w/`.
- **Proposed fix**: delete the five items and the three re-export names (`misc.rs:106`, `lib.rs:154`, `lib.rs:173`). Closed-issue cousin: #5745 (regression-adjacent, those five were added to its list in the 10-15 report but not fixed).

Related: #5745 (closed without these five items).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
