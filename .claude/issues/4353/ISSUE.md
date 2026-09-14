# #4353 — TD3-005: Other pre-split module paths in docs/engine

**Labels**: low, tech-debt, documentation, doc-rot
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4353

- **Severity**: LOW · **Dimension**: 3
- **Location**: `docs/engine/asset-pipeline.md:62,275` (broken links to *asset_provider/material.rs*), `docs/engine/coordinate-system.md:246` and `docs/engine/lighting-from-cells.md:57` (*references.rs*), `docs/engine/esm-records.md:531` (*actor.rs*) · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Suggested Fix**: Re-point to `material/provider.rs`, `material/merge.rs`, `cell_loader/references/` and `records/actor/`.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: If practical, a hygiene/gate check prevents this doc-rot class recurring
