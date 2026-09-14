# #4383 — TD8-004: Ground-cover dead code — `layer_affinities`' consumer claim is disproven; the Phase C helpers wait on an untracked design-doc phase

**Labels**: low, terrain-exterior, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4383

- **Severity**: LOW · **Dimension**: 8
- **Location**: `byroredux/src/groundcover_translate.rs:190` (`layer_affinities`), `:279` (`classify_species_name`), `:315` (`climate_weights_for`); stale doc `:59-69` · **Status**: NEW (related to closed #4226; not a regression) · **Age**: `637b65264` (09-06), `f8a900b3c` (09-13) · **Effort**: trivial · **Kind**: tech-debt
- **Finding**: Phase 1 shipped through the singular `layer_affinity` (`byroredux/src/cell_loader/terrain.rs:178,734`) and the shader's `byroGcAffinity`; the plural batch form has only a test caller. `f8a900b3c` removed the sole production caller of the two species helpers and added allows citing §12.12 Phase C, which has no issue (#3807 is the umbrella, #4056 is Phase 3). The DEFAULT_AFFINITY doc still calls the GPU affinity dispatch "pending".
- **Suggested Fix**: Delete `layer_affinities` and its test. File a Phase C tracker and cite it in the allows, or delete the helpers (recoverable from `f8a900b3c^`). Fix the `:59-69` doc.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
