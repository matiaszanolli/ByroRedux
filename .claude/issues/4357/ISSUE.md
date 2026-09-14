# #4357 — TD3-009: feature-matrix and ROADMAP still say AI package selection is spawn-time only

**Labels**: low, ai, tech-debt, documentation, doc-rot
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4357

- **Severity**: LOW · **Dimension**: 3
- **Location**: `docs/feature-matrix.md:93-96,324`, `ROADMAP.md:1217` · **Status**: NEW · **Age**: since M42.9 / #2652 · **Effort**: trivial · **Kind**: doc-rot
- **Finding**: `ambient_ai_package_system` is registered unconditionally (`byroredux/src/boot/schedule/update.rs:246`) and re-evaluates once per in-game minute. ROADMAP also lists combat as unbuilt.
- **Suggested Fix**: Update both status docs and drop the gap row.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: If practical, a hygiene/gate check prevents this doc-rot class recurring
