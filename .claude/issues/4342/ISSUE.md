# #4342 — TD1-006: #3858's census functions kept growing after closure — `render_one_frame` +148 LOC (+22%) in nine days

**Labels**: low, renderer, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4342

- **Severity**: LOW · **Dimension**: 1
- **Location**: `byroredux/src/app_frame.rs:49-861` (813 LOC, nesting 7); also `about_to_wait` (822), `record_skinned_blas_refit` (912), `collect_static_mesh_draws` (947) · **Status**: NEW (follow-up to closed #3858) · **Age**: `637b65264`, `40b5c5b6a`, `0025d8221`, `1a7a22cf0` · **Effort**: medium · **Kind**: tech-debt
- **Finding**: Each EXAL/SKYAL feature adds its per-frame hook inline. The ground-cover collection (`:274-391`) and Ruffle UI tick (`:392-535`) alone are ~260 lines.
- **Suggested Fix**: Extract `collect_groundcover_frame`, `tick_ui_overlay`, `drain_skin_slot_uploads`, `apply_pending_debug_requests` and `reconcile_post_draw_skin_state` as `App` methods **inside `byroredux/src/app_frame.rs`**: five `include_str!("app_frame.rs")` scans would go vacuous if the code moved files. File `record_skinned_blas_refit` / `collect_static_mesh_draws` separately with the RenderDoc caveat.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
