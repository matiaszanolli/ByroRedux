# Batch: 2036, 2037, 2038, 2039

Source: `docs/audits/AUDIT_PERFORMANCE_2026-07-16.md`

## #2036 — PERF-D4-01: upload_lights is the one per-frame SSBO without a content-hash dirty gate
- Severity: LOW · Dimension: SSBO Sizing & Upload
- Location: `crates/renderer/src/vulkan/scene_buffer/upload.rs:19-84`
- Instances (#1134), materials (#878), indirect draws (#1809) all gained a content-hash
  dirty-gate skip; `upload_lights` did not.
- Suggested fix: add a content-hash gate mirroring the instances/materials/indirect-draws
  pattern.
- Domain: renderer → `byroredux-renderer`

## #2037 — GPU-D5-01: Bloom upload_params rewrites construction-invariant UBOs every frame
- Severity: LOW · Dimension: GPU Pipeline
- Location: `crates/renderer/src/vulkan/bloom.rs:451-488`
- All 9 down/upsample param UBOs are pure functions of `self.extent`, fixed at
  construction, yet rewritten every frame (~144 bytes redundant host memcpy/frame).
- Suggested fix: write once at `BloomFrame::new` (and on resize, which already
  recreates the pipeline).
- Domain: renderer → `byroredux-renderer`

## #2038 — GPU-D5-INFO-01: Volumetrics casts ~1.8M per-froxel shadow rays/frame with no temporal reprojection yet
- Severity: LOW (informational — documented M55 Phase 5 roadmap gap, not a regression;
  explicitly "not actionable as a bug today")
- Location: `crates/renderer/src/vulkan/volumetrics.rs:916-919`,
  `crates/renderer/shaders/volumetrics_inject.comp:177-189`
- Suggested fix: none — "No action needed until M55 Phase 5 is scheduled. Tracked here
  so the cost lever isn't lost between audit passes."
- Domain: renderer → `byroredux-renderer` (tracking-only, verify ROADMAP.md reference)

## #2039 — PERF-D7-02: Cell-transition orchestrator discards warm material/texture caches on every door transition
- Severity: LOW · Dimension: Streaming & Cells
- Location: `byroredux/src/app_step.rs:255-298` (`step_cell_transition`),
  `byroredux/src/save_io.rs:610-614`
- `build_material_provider`/`build_texture_provider` called fresh on every transition,
  discarding BGSM/BGEM template cache, `csg_cache`, `sf_cdbs`. Low-impact today since
  `PendingCellTransition` is only reachable via the `door.teleport` console command
  (Stage 4 interactive door activation hasn't shipped).
- Suggested fix: "Worth a design note now (cache providers across transitions, keyed by
  loaded-plugin-set identity) so it's ready before Stage 4 door activation lands; no
  urgency before then." — design note, not full implementation.
- Domain: binary → `byroredux`
