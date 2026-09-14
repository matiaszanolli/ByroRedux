# #4361 — TD3-013: `clouds.glsl` header and march comment still describe the fixed-step march the adaptive march replaced

**Labels**: low, shaders, terrain-exterior, tech-debt, documentation, doc-rot
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4361

- **Severity**: LOW · **Dimension**: 3
- **Location**: `crates/renderer/shaders/include/clouds.glsl:54` (lists *_VIEW_STEPS*), `:202-203` ("48 steps do not band") · **Status**: NEW (Dim 7 hand-off, orchestrator-verified) · **Age**: `9ac8a9299` (09-13) · **Effort**: trivial · **Kind**: doc-rot
- **Finding**: `shader_constants_data.rs` now defines `CLOUD_CHEAP_SAMPLES_ZENITH = 64` / `CLOUD_CHEAP_SAMPLES_HORIZON = 128`, and no `CLOUD_VIEW_STEPS` exists.
- **Suggested Fix**: Name the cheap-sample constants and drop the step count.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: If practical, a hygiene/gate check prevents this doc-rot class recurring
