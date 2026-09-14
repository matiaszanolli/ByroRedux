# #4351 — TD3-003: CLAUDE.md's Workspace Structure names three files that were split into directories

**Labels**: low, tech-debt, documentation, doc-rot
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4351

- **Severity**: LOW · **Dimension**: 3
- **Location**: `CLAUDE.md:68` (*asset_provider/material.rs*), `:70` (*asset_provider/tests.rs*), `:152` (*texture_registry.rs*) · **Status**: NEW · **Age**: `42f0ead40` (09-09), `196a2faa3` (09-11) · **Effort**: trivial · **Kind**: doc-rot
- **Suggested Fix**: Point them at `byroredux/src/asset_provider/material/`, `byroredux/src/asset_provider/tests/` and `crates/renderer/src/texture_registry/`. The tree is fenced, so the gate can't see it (TD4-005).

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: If practical, a hygiene/gate check prevents this doc-rot class recurring
