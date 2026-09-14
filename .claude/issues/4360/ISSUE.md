# #4360 — TD3-012: docs/engine describes deleted or renamed code symbols as live

**Labels**: low, tech-debt, documentation, doc-rot
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4360

- **Severity**: LOW · **Dimension**: 3
- **Location**: `docs/engine/scripting.md:116,600` (*quest_advance_on_activate_system* → `quest_advance_system`), `docs/engine/animation.md:352` (deleted AnimationController's *resolve_blend_time*), `docs/engine/exal.md:632` (*bto_archive_path* → `object_lod_archive_path`), `docs/engine/renderer.md:312` (per-draw push constants that no longer exist; data comes via `gl_InstanceIndex`), `docs/engine/stream-boundary-state-continuity.md:187` (deleted *persistent_ref_index.rs*), `docs/engine/watal.md:134` (removed *MAX_CELLS_SPAWNED_PER_FRAME*) · **Status**: NEW · **Effort**: small · **Kind**: doc-rot

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: If practical, a hygiene/gate check prevents this doc-rot class recurring
