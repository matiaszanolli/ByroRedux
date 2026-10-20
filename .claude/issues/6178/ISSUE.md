# TD4-2026-10-19-06: `audit-scripting/SKILL.md:32` "~92k LOC incl. tests" for `crates/scripting/src/` is now 110.9k (+20.5%)

Labels: low,documentation,doc-rot,tech-debt
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW | **Dimension**: 4 | **Effort**: trivial
- **Evidence**: `find crates/scripting/src -name '*.rs' | xargs cat | wc -l` = 110,872 (the line itself says re-measure). Other sampled figures hold: `crates/plugin/src` ~73k (75.6k), `crates/save/src` ~3.2k (3,208), `crates/spt/src` ~2.8k (2,841), `byroredux/src/extensions` ~10.8k (10,662), `papyrus_provider/`+`compatibility.rs` ~7.5k (8.7k, +16%, under threshold), `cxx-bridge` 26-line lib.rs + 9-line C++ stub, `platform` 60 LOC, `GpuMaterial` 432 B, 4112 B light header, 142 Skyrim catalog entries, "default all N" dimension counts for all 23 skills that state one.
- **Proposed fix**: bump to ~111k or drop the figure.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in the neighbouring docs / skills
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
