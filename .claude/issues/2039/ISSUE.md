# PERF-D7-02: Cell-transition orchestrator discards warm material/texture caches on every door transition

**Labels**: low, performance, bug

**Severity**: LOW
**Dimension**: Streaming & Cells
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-16.md`

## Location
`byroredux/src/app_step.rs:255-298` (`step_cell_transition`), `byroredux/src/save_io.rs:610-614`

## Description
`build_material_provider`/`build_texture_provider` are called fresh on every transition, discarding the BGSM/BGEM template cache, `csg_cache`, and `sf_cdbs`. Currently low-impact: `PendingCellTransition` is only queued by the `door.teleport` **console command** — interactive door activation (Stage 4) hasn't shipped yet. Will become a real per-door gameplay cost once it does.

Verified current: `step_cell_transition` (`byroredux/src/app_step.rs:255-298`) still calls `crate::asset_provider::build_texture_provider(&args)` and `crate::asset_provider::build_material_provider(&args)` fresh on every transition; the function's own doc comment acknowledges "the cost is a few-hundred-ms BSA re-open per transition, acceptable for the single-trigger door flow."

## Impact
No urgency before interactive door activation (Stage 4) ships — currently only reachable via the `door.teleport` console command, not real gameplay.

## Suggested Fix
Worth a design note now (cache providers across transitions, keyed by loaded-plugin-set identity) so it's ready before Stage 4 door activation lands; no urgency before then.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix once provider caching is implemented (currently not urgent)
