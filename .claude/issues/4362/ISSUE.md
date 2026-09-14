# #4362 — TD4-001: The tech-debt skill's Phase-1 orientation asserts false facts: "`shader.rs` dropped back under — do not re-propose", "2 files", "every monolith is split"

**Labels**: low, tech-debt, documentation, doc-rot
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4362

- **Severity**: LOW · **Dimension**: 4
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md:137-155,180,184,192,215` · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot · **Related**: TD1-001
- **Finding**: `crates/nif/src/blocks/shader.rs` was never split and re-crossed on 2026-09-12 (2058 production, string-aware), so a do-not-re-propose instruction covers a live candidate. The primary bucket is 3 files, not 2 (the "4" the skill's own awk reports includes the TD1-001 false positive). `:180` "11 files" contradicts `:138`. `:192` says `context/` has 19 files; it has 18. The secondary bucket is 43, not 40.
- **Suggested Fix**: Reword the shader.rs clause as dated history, drop the count and "every monolith" sentences in favour of "re-run the recipe", and fix 19→18. Do this after TD1-001's counter fix so the new figures are real.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: If practical, a hygiene/gate check prevents this doc-rot class recurring
