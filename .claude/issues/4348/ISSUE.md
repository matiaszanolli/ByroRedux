# #4348 — TD2-006: Ground-cover scatter and its bench carry byte-identical candidate generators

**Labels**: low, shaders, terrain-exterior, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4348

- **Severity**: LOW · **Dimension**: 2
- **Location**: `crates/renderer/shaders/groundcover_scatter.comp:155-169` (`gcCandidate`), `crates/renderer/shaders/include/groundcover_bench.glsl:148-164` (`benchCandidate`) · **Status**: NEW · **Age**: `637b65264` / `40b5c5b6a` (09-06) · **Effort**: trivial · **Kind**: tech-debt
- **Finding**: The bench exists to time the production distribution, but it re-types it. #4057 already moved the Laplacian into a shared include for the same reason.
- **Suggested Fix**: A guarded *include/groundcover_candidate.glsl* (new) with `byroGcCandidate`, included by both.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
