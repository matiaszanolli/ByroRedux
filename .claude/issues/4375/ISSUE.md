# #4375 — TD6-008: `AnimationClip.phase` doc says "NOT yet consumed … no field exists"; #3345 wired it

**Labels**: low, animation, nif, tech-debt, documentation, doc-rot
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4375

- **Severity**: LOW · **Dimension**: 6
- **Location**: `crates/nif/src/anim/types.rs:165-174` · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Finding**: `AnimationPlayer::with_phase` is called at `byroredux/src/scene/nif_loader.rs:647`, `byroredux/src/cell_loader/spawn.rs:840` and `byroredux/src/scene.rs:1183`.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: If practical, a hygiene/gate check prevents this doc-rot class recurring
