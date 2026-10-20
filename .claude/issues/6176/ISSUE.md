# TD4-2026-10-19-04: `audit-tech-debt/SKILL.md` baseline paragraph (lines 81-97) is stale in the same split-driven way the brief predicted

Labels: low,documentation,doc-rot,tech-debt
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW | **Dimension**: 4 | **Effort**: trivial | **Age**: written 2026-10-15 at `f4ae42a6f`
- **Evidence** (today's `baseline.txt` / `prodloc_all.txt`): ">2000 production LOC: **6**" is now **4**: `shader_constants_data.rs` 2868 (data table), `scripting/examples/mq101_conformance.rs` 2157, `audio/src/lib.rs` 2034, `nif/src/import/types.rs` 2001. The five named large files are now hubs: `fragment/vm.rs` 134 (text says 6213, "a single file"), `translate/effects.rs` 97 (5295), `save_io.rs` 86 (2136), `esm/cell/mod.rs` 68 (2108), `condition.rs` 123 (2005). Watch-list rows crossed the line instead: types.rs 1997->2001, mq101 1974->2157, audio 1977->2034 (these are Dim 1 findings). `#[ignore]` 322 -> 343, markers 24 -> 23, test-heavy >2000 84 -> 86; `allow(dead_code)` 41 and `todo!` 0 unchanged.
- **Proposed fix**: replace the measured-numbers sentence with today's snapshot, or (better, it already says "never quote") delete the per-file LOC list and keep only the directional counts plus the recipe; note the split-hub hazard from TD4-02 next to the Dim 1 recipe.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in the neighbouring docs / skills
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
