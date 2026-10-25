# TD4-2026-10-25-04: `audit-character` says the sub-module count is 15 and "pinned"; the pin says 16

Labels: low, doc-rot, character, tech-debt, documentation
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6384
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Dimension**: 4 · **Effort**: trivial
- **Location**: `.claude/commands/audit-character/SKILL.md:20` vs `crates/core/src/character/mod.rs:161-165`
- **Evidence**: skill: "`crates/core/src/character/` (15 modules, `mod_docstring_indexes_every_sub_module` pins the count)". The test asserts `declared.len() == 16`; `ls` shows 16 sub-modules plus `mod.rs` (affliction, attribute, components, crime, derived, fallout, leveling, profile, regen, reputation, resistance, ruleset, skill, skyrim, tes, vats).
- **Suggested Fix**: "16 modules". (6% drift; filed because it contradicts the very guard it cites.)

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD4-2026-10-25-04`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
