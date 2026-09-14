# #4367 — TD4-006: `_audit-common.md` still says the feature-matrix M45/M47 rows lag; they were fixed 2026-06-21

**Labels**: low, tech-debt, documentation, doc-rot
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4367

- **Severity**: LOW · **Dimension**: 4
- **Location**: `.claude/commands/_audit-common.md:132` · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Finding**: This contradicts the tech-debt skill's own Dim 3 note. Two audits already cited it as a premise (`docs/audits/AUDIT_FO3_2026-08-27.md:184`, `docs/audits/AUDIT_CHARACTER_2026-08-30.md:675`), though neither re-filed.
- **Suggested Fix**: Replace with "status floor; re-check each row against its crate".

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: If practical, a hygiene/gate check prevents this doc-rot class recurring
