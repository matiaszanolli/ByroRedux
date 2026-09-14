# #4371 — TD4-011: `audit-renderer` Dim 18 still says "fog applied to direct only (Dim 8)", the premise #4047 removed from Dim 8

**Labels**: low, tech-debt, documentation, doc-rot
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4371

- **Severity**: LOW · **Dimension**: 4
- **Location**: `.claude/commands/audit-renderer/SKILL.md:299` · **Status**: NEW (unfixed sibling of closed #4047) · **Effort**: trivial · **Kind**: doc-rot
- **Suggested Fix**: "fog/volumetric transmittance applied to the combined direct+indirect term, pre-tonemap".

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: If practical, a hygiene/gate check prevents this doc-rot class recurring
